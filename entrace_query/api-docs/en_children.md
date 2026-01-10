Get the children of an entry.

## INPUT
A span id.

## OUTPUT
The list of children (list[int]).

## EXAMPLE
local children = en_children(id)
for i, child_id in ipairs(children) do
  en_log(child_id)
end
