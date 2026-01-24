Gather a result list from the current span range by executing a callback for each span.

## INPUT
A callback function `f: u32 -> nil | boolean | integer | table`.

The callback can return:
- `nil`: Nothing is added to the results.
- `boolean`: if true, the span id the callback was called with is added to the results
- `integer`: if convertible to u32, it's added to the results.
- `table`: An array of integers to be added to the results.

## OUTPUT
A list of span IDs (list[int]).

## EXAMPLE
The following two implementations are equivalent: 
-- Return all spans that contain the word "foobar"
local results = en_foreach(function(i) 
  return en_contains_anywhere(i, "foobar")
end)

-- this is equivalent to this code:
local results = {}
local rstart,rend = en_span_range()

for i=rstart,rend do
	if en_contains_anywhere(i, "foobar") then
    	table.insert(results, i)
	end
end
