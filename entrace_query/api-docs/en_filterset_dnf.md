Apply a filter in Disjunctive Normal Form (OR of ANDs) to a filterset.

## INPUT
- A clause list, where each clause is a list of filter descriptions accepted by en_filter.
Such a filter description is a table that looks like:
  - target: name of variable, eg. "message" or "meta.filename"
  - relation: a string, one of "EQ" | "LT" | "GT"
  - value: a constant to compare with.
- A source filterset.

## OUTPUT
A new filterset matching spans which satisfy the DNF.

## EXAMPLE
local fs = en_filterset_from_range(0, 100)
local level_5_or_message_error = en_filterset_dnf({
  { {target="meta.level", relation="EQ", value=5} },
  { {target="message", relation="EQ", value="error"} }
}, fs)