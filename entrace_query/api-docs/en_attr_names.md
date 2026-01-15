Get a list of attribute names for an entry.

## INPUT
A span id.

## OUTPUT
list[string] of attribute names.

## EXAMPLE
local names = en_attr_names(id)
for i, name in ipairs(names) do
  en_log(name)
end
