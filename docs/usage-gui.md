# Using the ENTRACE GUI
## Loading a trace
Use the `File` menu to load a trace from a file, or to start a TCP server to wich the ENTRACE client library can connect.

## Navigating traces
The trace is laid out in a nested fashion, following `tracing`'s model of inter-contained spans.

By default, all spans are closed; spans can be opened by clicking on the header.
ENTRACE automatically registers a root span, where spans that have no parent are registered.

## Converting traces
ENTRACE provides a way to convert between `et` and `iet` files using the GUI.
Open the convert dialog from the menu by `Tools` -> `Convert`.

## Querying traces
Filtering traces is perhaps the primary function of the ENTRACE system.
The ENTRACE GUI provides a convenient way to accomplish this task.

It is important to note that the current query system is provided by the GUI, not `entrace_core`, for more flexibility, but this may change in the future.

### Anatomy of a query
Instead of a custom Domain-Specific Language, ENTRACE provides **a Lua-based API** for querying traces.
The GUI budles a Lua interpreter (`luajit`), which executes the code entered into the bottom panel when pressing <kbd>Ctrl+Enter</kbd>, or clicking the Run (`▶`) button.

The query process is as follows:

- User enters query, and clicks Run.
- ENTRACE spawns multiple threads, which will process the query.
The number of threads spawned can be configured by clicking the cogwheel icon next to Run. The default number of threads is equal to the CPU (virtual) core count.
- ENTRACE partitions the full range of spans into separate batches (span ranges) for threads.
- The threads execute the query on the spans defined by the span range for the thread.
- ENTRACE aggregates all results, and displays them to the user.

### Writing a query
In ENTRACE, a query is essentially **a Lua block, which returns a list** (table) of span IDs selected by the query.

A simple query SHOULD look like this:
```lua
local r_start, r_end = en_span_range()
local ids = {}
for i = r_start, r_end do
    -- work with the current span here
    -- eg, for a query that returns everything:
    table.insert(ids, i)
end
return ids
```

The methods to work with the query data are likewise prefixed by `en_`.

### Example queries
There are some example queries in [`example_query.lua`](../example_query.lua) in the repo root. 
### Available API
The full API documentation for ENTRACE can be found in the [search module docs](../gui/src/search/lua_api.rs).
Every function whose name starts with `en_` here implements a lua method for queries.

### Disabling parallelism
You can disable parallelism by setting the query thread count to 0, but this is not recommended, as it degrades performance.

### Jumping to an entry in the main tree
You can jump to a returned span in the main tree by right-clicking it in the query result view, and choosing "Locate in main tree". 
