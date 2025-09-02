use std::{
    env,
    fmt::{Debug, Display},
    fs::{File, OpenOptions},
    io::{BufReader, BufWriter, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

use anyhow::{Context, bail};
use crossbeam::channel::Receiver;
use egui::RichText;
use entrace_core::{convert::ConvertError, display_error_context};
use rfd::FileDialog;
use tracing::{trace, warn};

use crate::{App, settings::left_stroke_frame, spawn_task, time_print};

#[derive(Default)]
pub enum ConvertDialogState {
    #[default]
    NotOpen,
    Open(ConvertDialogStateInner),
}
#[derive(Debug)]
pub enum ConvertState {
    NotStarted,
    Converting { rx: Receiver<(Duration, Result<(), ConvertError>)> },
    Done(Duration),
}
struct ConvertFilePath {
    path: Option<PathBuf>,
    ty: ConvertFileType,
}
impl Default for ConvertFilePath {
    fn default() -> Self {
        Self { path: None, ty: ConvertFileType::IET }
    }
}

pub struct ConvertDialogStateInner {
    convert_state: ConvertState,
    error: Option<ConvertDialogError>,
    input: ConvertFilePath,
    output: ConvertFilePath,
}

impl Default for ConvertDialogStateInner {
    fn default() -> Self {
        Self {
            convert_state: ConvertState::NotStarted,
            input: ConvertFilePath::default(),
            output: ConvertFilePath { path: None, ty: ConvertFileType::ET },
            error: None,
        }
    }
}

#[derive(PartialEq)]
pub enum ConvertFileType {
    ET,
    IET,
}
impl Display for ConvertFileType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConvertFileType::ET => write!(f, "ET"),
            ConvertFileType::IET => write!(f, "IET"),
        }
    }
}
struct ConvertDialogError {
    header: String,
    body: String,
}
pub fn convert_dialog(ui: &mut egui::Ui, app: &mut App) {
    let ConvertDialogState::Open(ref mut inner) = app.convert_dialog else {
        return;
    };
    let mut open = true;
    egui::Window::new("Convert").open(&mut open).show(ui.ctx(), |ui| {
        ui.label("Input");
        left_stroke_frame(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("File:");
                if ui.button("Pick").clicked() {
                    let mut files = FileDialog::new();
                    if let Ok(x) = env::current_dir() {
                        files = files.set_directory(x)
                    }
                    if let Some(picked) = files.pick_file() {
                        inner.input.path = Some(picked);
                    }
                }
            });
            if let Some(q) = &inner.input.path {
                ui.horizontal(|ui| {
                    ui.label("Path:");
                    ui.code(q.display().to_string());
                });
            }
            ui.horizontal(|ui| {
                ui.label("Type:");
                egui::ComboBox::from_id_salt("convert_src_type")
                    .selected_text(format!("{}", inner.input.ty))
                    .show_ui(ui, |ui| {
                        use ConvertFileType::*;
                        for value in [ET, IET] {
                            let repr = value.to_string();
                            ui.selectable_value(&mut inner.input.ty, value, repr);
                        }
                    });
            });
        });
        ui.add_space(ui.spacing().item_spacing.y * 2.0);
        ui.label("Output");
        left_stroke_frame(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("File:");

                if ui.button("Pick").clicked() {
                    let mut files = FileDialog::new();
                    if let Ok(x) = env::current_dir() {
                        files = files.set_directory(x)
                    }
                    if let Some(picked) = files.pick_file() {
                        inner.output.path = Some(picked);
                    }
                }
            });
            if let Some(q) = &inner.output.path {
                ui.horizontal(|ui| {
                    ui.label("Path:");
                    ui.code(q.display().to_string());
                });
            }
            ui.horizontal(|ui| {
                ui.label("Type:");
                egui::ComboBox::from_id_salt("convert_to_type")
                    .selected_text(format!("{}", inner.output.ty))
                    .show_ui(ui, |ui| {
                        use ConvertFileType::*;
                        for value in [ET, IET] {
                            let repr = value.to_string();
                            ui.selectable_value(&mut inner.output.ty, value, repr);
                        }
                    });
            });
        });
        if let Some(ref y) = inner.error {
            ui.collapsing(RichText::new(&y.header).color(ui.visuals().error_fg_color), |ui| {
                ui.label(RichText::new(&y.body).color(ui.visuals().error_fg_color));
            });
        }
        match inner.convert_state {
            ConvertState::NotStarted => (),
            ConvertState::Converting { ref rx } => match rx.try_recv() {
                Ok((elapsed, d)) => match d {
                    Ok(_) => inner.convert_state = ConvertState::Done(elapsed),
                    Err(y) => {
                        let formatted = display_error_context(&y);
                        tracing::error!(error = formatted, "convert: got error");
                        inner.error =
                            Some(ConvertDialogError { header: format!("{y:?}"), body: formatted });
                        inner.convert_state = ConvertState::Done(elapsed);
                    }
                },
                Err(y) => match y {
                    crossbeam::channel::TryRecvError::Empty => {
                        ui.spinner();
                    }
                    crossbeam::channel::TryRecvError::Disconnected => {
                        warn!("convert: channel disconnect");
                        inner.error = Some(ConvertDialogError {
                            header: format!("{y:?}"),
                            body: display_error_context(&y),
                        });
                    }
                },
            },
            ConvertState::Done(dur) => {
                ui.horizontal(|ui| {
                    ui.label("Done in");
                    ui.label(format!("{dur:?}"));
                });
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            if ui.button("Convert").clicked() {
                trace!("Starting convert!");
                inner.error = None;
                match dispatch_convert(inner) {
                    Err(y) => {
                        inner.error = Some(ConvertDialogError {
                            header: format!("{y:?}"),
                            body: display_error_context(&*y.into_boxed_dyn_error()),
                        })
                    }
                    Ok(rx) => inner.convert_state = ConvertState::Converting { rx },
                }
            }
        });
    });
    if !open {
        app.convert_dialog = ConvertDialogState::NotOpen;
    }
}
#[allow(clippy::type_complexity)]
pub fn dispatch_convert(
    inner: &mut ConvertDialogStateInner,
) -> Result<Receiver<(Duration, Result<(), ConvertError>)>, anyhow::Error> {
    use ConvertFileType::*;
    fn setup_io(
        in_path: &PathBuf, out_path: &PathBuf,
    ) -> Result<(BufReader<File>, BufWriter<File>), ConvertError> {
        let in_reader = File::open(in_path).map_err(ConvertError::ReadInputError)?;
        let out_writer = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(out_path)
            .map_err(ConvertError::OutWriteError)?;
        let in_reader = BufReader::new(in_reader);
        let out_writer = BufWriter::new(out_writer);
        Ok((in_reader, out_writer))
    }

    let input_path = inner.input.path.clone().context("No input file")?;
    let output_path = inner.output.path.clone().context("No output file")?;

    match (&inner.input.ty, &inner.output.ty) {
        (ET, ET) | (IET, IET) => bail!("Can't convert from and to the same file type"),
        (ConvertFileType::ET, ConvertFileType::IET) => {
            let (tx, rx) = crossbeam::channel::bounded::<(Duration, Result<(), ConvertError>)>(1);
            spawn_task(move || {
                let start = Instant::now();
                let (mut in_reader, mut out_writer) = match setup_io(&input_path, &output_path) {
                    Ok((x, y)) => (x, y),
                    Err(y) => {
                        tx.send((start.elapsed(), Err(y))).ok();
                        return;
                    }
                };
                let r = time_print("ht_to_iht", || {
                    entrace_core::convert::et_to_iet(&mut in_reader, &mut out_writer, true)
                })
                .and_then(|_| out_writer.flush().map_err(ConvertError::OutWriteError));
                tx.send((start.elapsed(), r)).ok();
            });
            Ok(rx)
        }
        (ConvertFileType::IET, ConvertFileType::ET) => {
            let (tx, rx) = crossbeam::channel::bounded(1);
            spawn_task(move || {
                let start = Instant::now();
                let (mut in_reader, mut out_writer) = match setup_io(&input_path, &output_path) {
                    Ok((x, y)) => (x, y),
                    Err(y) => {
                        tx.send((start.elapsed(), Err(y))).ok();
                        return;
                    }
                };
                let r = time_print("iht_to_ht", || {
                    entrace_core::convert::iet_to_et(&mut in_reader, &mut out_writer, true, false)
                })
                .and_then(|_| out_writer.flush().map_err(ConvertError::OutWriteError));
                tx.send((start.elapsed(), r)).ok();
            });
            Ok(rx)
        }
    }
}
