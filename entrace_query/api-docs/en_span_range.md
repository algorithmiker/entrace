Returns the current thread's span range.

ENTRACE runs your query on multiple threads, by dividing the searchable span range equally to multiple threads. The number of threads can be configured in the GUI.

## INPUT
Nothing.

## OUTPUT
A pair of integers.

## EXAMPLE
local rstart,rend = en_span_range()
