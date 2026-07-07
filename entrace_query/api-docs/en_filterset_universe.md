A shortcut for `en_filterset_from_range(en_span_range())`.
See also the `en_span_range` docs.

## INPUT
Nothing.

## OUTPUT
A filterset matching all spans in `en_span_range()`

## EXAMPLE
-- matches all spans with message="Hello World" from the whole trace
en_filter("message", "EQ", "Hello World", en_filterset_universe())
