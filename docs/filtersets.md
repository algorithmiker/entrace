THE FILTERSET CALCULUS (v2)
---

### TYPES

```rust
type FiltersetId = usize;
type PreidcateId = usize;
struct Predicate {
    attr: String,
    rel: Ordering,
    con: EnValue,
}
#[derive(Debug)]
enum Filterset {
    Dead,
    Primitive(Roaring),
    BlackBox(FiltersetId),
    RelDnf(Vec<Vec<PredicateId>>, FiltersetId),
    And(Vec<FiltersetId>),
    Or(Vec<FiltersetId>),
    Not(FiltersetId),
}
```

### REWRITE RULES

**Flattening**

And([... And(children) ...]) -> And(flattened)
Or([... Or(children) ...])  -> Or(flattened)
Not(Not(X)) -> X
And([A]) -> A
Or([A]) -> A

**One Rule To Rule Them All**
1. RelDnf(clauses, RelDnf(clauses2, A)) -> new clauses: c_1 \times c_2 (if the result won't be too big)

**RelDnf in Or/And**
1. Or([RelDnf(c, A), RelDnf(c2, A), RelDnf(c3, B), RelDnf(c4, B)]) -> Or([RelDnf(c+c2, A), RelDnf(c3+c4, B)])
2. And([RelDnf(c, A), RelDnf(c2, A), RelDnf(c3, B), RelDnf(c4, B)]) -> And([RelDnf(c x c2, A), RelDnf(c3 x c4, B)]) (if not too big)
