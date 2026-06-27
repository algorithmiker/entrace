Invert a filterset with respect to an _universe.

## INPUT
A filterset f and an universe u.

## OUTPUT
A filterset that matches all spans not matched by the input filterset, but matched by the universe, i. e. u \ f.

## EXAMPLE
local root = en_filterset_from_range(1, 6)
local eq = en_filter("c", "EQ", 0, root)
local ge = en_filter("c", "GE", 0, root)
local strictly_greater = en_filterset_invert(eq, ge)
