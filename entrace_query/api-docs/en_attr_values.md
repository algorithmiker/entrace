Get a list of attribute values for an entry.

## INPUT
A span id.

## OUTPUT
list[object]: a list of attribute values.

## EXAMPLE
local values = en_attr_values(id)
for i, value in ipairs(values) do
  en_log(value)
end
