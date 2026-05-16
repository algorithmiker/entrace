Apply a filter to a filterset to match only the spans matching a relation.

## INPUT
This function has two possible signatures:
1. `en_filter(filter: Table, src: Table) -> Table`
  - filter: A table describing the relation:
    - target: name of variable, eg. "message" or "meta.filename"
    - relation: a string, one of "EQ" | "LT" | "GT"
    - value: a constant to compare with.
  - src: a filterset.
2. `en_filter(target: String, relation: String, value: T, src: Table) -> Table`
  This is basically the same, but with the table fields from above unpacked conveniently

## OUTPUT
A filterset that matches only the spans which satisfy the relation.

## EXAMPLE
local fs = en_filterset_from_range(0, 100)
local filtered = en_filter({target = "meta.level", relation = "EQ", value = 5}, fs)
-- equivalent: 
local filtered = en_filter("meta.level", "EQ", 5, fs)
