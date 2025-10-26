THE FILTERSET CALCULUS
---

### TYPES

```rust
type FiltersetId = usize;
struct Predicate {
    attr: String,
    rel: Ordering,
    con: EnValue,
}
#[derive(Debug)]
enum Filterset {
    Dead,
    Primitive(Roaring),
    Rel(Predicate, FiltersetId),
    RelIntersect(Vec<Predicate>, FiltersetId),
    RelUnion(Vec<Predicate>, FiltersetId),
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

**Eliminating trivial ops (unimplemeneted)**
And([A]) -> A
Or([A]) -> A

**REL composition**

I.   Rel(p1, Rel(p2, A)) -> RelIntersect([p1, p2], A)
II.  Rel(p, RelIntersect(ps, A)) -> RelIntersect([p] + ps, A)
III. RelIntersect(ps1, RelIntersect(ps2, A)) -> RelIntersect(ps1 + ps2, A)
IV.  RelIntersect(ps, Rel(p, A)) -> RelIntersect(ps + [p], A)
V.   RelUnion(ps1, RelUnion(ps2, A)) -> RelUnion(ps1 + ps2, A)


**Flattening on the same level (unimplemeneted)**
And([RelIntersect(ps, A), RelIntersect(ps2, A), RelIntersect(_, B) ...])
  -> And([RelIntersect(ps1+ps2, A), RelIntersect(_, B)])
    - This could be hard to detect, instead there could be just:
      And([RelIntersect(ps_1, A), .. RelIntersect(ps_n, A)]) -> RelIntersect(ps_1+..+ps_n, A)
Or([RelUnion(ps, A), RelUnion(ps2, A), RelUnion(_, B) ...])
    -> Or([RelUnion(ps1+ps2, A), RelIntersect(_, B)])
    - same applies here.
