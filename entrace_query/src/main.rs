use std::cmp::Ordering;

use entrace_core::EnValue;
use entrace_query::filtersets::{Evaluator, Filterset, Predicate};
use roaring::RoaringBitmap as Roaring;
fn main() {
    // Motivating example: filter people with (180<height<195 and 75<weight<90) or (iq == 120)
    let mut evaluator = Evaluator::<EnValue>::new();
    use EnValue::*;
    use Ordering::*;
    let src = evaluator.new_filterset(Filterset::Primitive(Roaring::full()));
    let height_lower =
        evaluator.new_dnf(vec![vec![Predicate::new("height", Greater, U64(180))]], src);
    let height_upper = evaluator.new_dnf(vec![vec![Predicate::new("height", Less, U64(195))]], src);
    let height_and = evaluator.new_filterset(Filterset::And(vec![height_lower, height_upper]));
    let weight_lower =
        evaluator.new_dnf(vec![vec![Predicate::new("weight", Greater, U64(75))]], height_and);
    let weight_upper =
        evaluator.new_dnf(vec![vec![Predicate::new("weight", Less, U64(90))]], weight_lower);
    let iq = evaluator.new_dnf(vec![vec![Predicate::new("iq", Equal, U64(120))]], 0);
    let or = evaluator.new_filterset(Filterset::Or(vec![weight_upper, iq]));
    println!("Before:\n{}", evaluator.dot(or));
    evaluator.normalize(or);
    println!("After:\n{}", evaluator.dot(or));
}
