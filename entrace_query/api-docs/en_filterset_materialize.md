Materialize a filterset into a list of matching span IDs. 
In some lazy languages, this operation is called *force*.

## INPUT
A filterset.

## OUTPUT
A list (sequence table) of span IDs.

## EXAMPLE
local fs = en_filterset_from_range(0, 100)
local ids = en_filterset_materialize(fs)
for i, id in ipairs(ids) do
  en_log(id)
end