pub struct EphemeralSettings {
    pub fps_in_menu: bool,
    pub demo_mode: bool,
}

#[allow(clippy::derivable_impls)]
impl Default for EphemeralSettings {
    fn default() -> Self {
        Self { fps_in_menu: false, demo_mode: false }
    }
}
