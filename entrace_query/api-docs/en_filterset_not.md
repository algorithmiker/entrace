Invert a filterset.

## INPUT
A filterset.

## OUTPUT
A filterset that matches all spans not matched by the input filterset.

## EXAMPLE
local fs = en_filterset_from_range(0, 100)
local inverted = en_filterset_not(fs)