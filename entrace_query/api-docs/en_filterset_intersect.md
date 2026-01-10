Create a filterset that matches the intersection of multiple filtersets.

## INPUT
A list of filtersets.

## OUTPUT
A filterset that matches a span only if it is matched by all of the input filtersets.

## EXAMPLE
local fs1 = en_filterset_from_range(0, 100)
local fs2 = en_filterset_from_range(50, 150)
local combined = en_filterset_intersect({fs1, fs2})