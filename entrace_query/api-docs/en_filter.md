Apply a filter to a filterset to match only the spans matching a relation.

## INPUT
- A table describing the relation:
  - target: name of variable, eg. "message" or "meta.filename"
  - relation: a string, one of "EQ" | "LT" | "GT"
  - value: a constant to compare with.
- src: a filterset.

## OUTPUT
A filterset that matches only the spans which satisfy the relation.

## EXAMPLE
local fs = en_filterset_from_range(0, 100)
local filtered = en_filter({target = "meta.level", relation = "EQ", value = 5}, fs)