use std::cmp::Ordering;

use entrace_core::EnValue;
use entrace_query::filtersets::{Evaluator, Filterset, Predicate, YesManMatcher};
use roaring::RoaringBitmap as Roaring;
fn main() {
    // Motivating example: filter people with (180<height<195 and 75<weight<90) or (iq == 120)
    let mut evaluator = Evaluator::<YesManMatcher, EnValue>::from_matcher(YesManMatcher());
    evaluator.pool.push(Filterset::Primitive(Roaring::full()));
    evaluator
        .pool
        .push(Filterset::Rel(Predicate::new("height", Ordering::Greater, EnValue::U64(180)), 0));
    evaluator
        .pool
        .push(Filterset::Rel(Predicate::new("height", Ordering::Less, EnValue::U64(195)), 1));
    evaluator
        .pool
        .push(Filterset::Rel(Predicate::new("weight", Ordering::Greater, EnValue::U64(75)), 2));
    evaluator
        .pool
        .push(Filterset::Rel(Predicate::new("weight", Ordering::Less, EnValue::U64(90)), 3));
    evaluator
        .pool
        .push(Filterset::Rel(Predicate::new("iq", Ordering::Equal, EnValue::U64(120)), 0));
    evaluator.pool.push(Filterset::Or(vec![4, 5]));
    println!("Before:\n{}", evaluator.dot(6));
    evaluator.normalize(6);
    println!("After:\n{}", evaluator.dot(6));
}
