Create a filterset that matches the union of multiple filtersets.

## INPUT
A list of filtersets.

## OUTPUT
A filterset that matches a span if it is matched by any of the input filtersets.

## EXAMPLE
local fs1 = en_filterset_from_range(0, 10)
local fs2 = en_filterset_from_range(20, 30)
local combined = en_filterset_union({fs1, fs2})