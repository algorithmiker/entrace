Switch from N threads to one thread.

All query threads that reach the en_join point are shut down, except for the last one. 
This last thread receives a concatenated list of IDs submitted by all threads.

This is useful for map-reduce type computations where the first part of the operation can be
parallelized, but we need serial execution on the last part;
for example if you want to sort the returned spans.

## INPUT
A list (sequence table) of span IDs.

## OUTPUT
A combined list of span IDs if this is the last thread to finish, otherwise the thread shuts down.

## EXAMPLE
local rstart, rend = en_span_range()
local ids = {}
for i=rstart,rend do
    table.insert(ids,i)
end
local total_ids = en_join(ids)

-- Sort the combined results by name
table.sort(total_ids, function(a, b)
  return en_metadata_name(a) < en_metadata_name(b)
end)

return total_ids