Create a filterset from a range of span IDs.

## INPUT
- start index (int)
- end index (int)

## OUTPUT
A filterset matching all spans in [start, end].

## EXAMPLE
local fs = en_filterset_from_range(0, 100)