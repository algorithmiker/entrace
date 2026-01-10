Check if the string representation of an entry contains a given substring.

## INPUT
- A span id.
- The substring to search for.

## OUTPUT
Whether the substring appears in the entry.

## EXAMPLE
if en_contains_anywhere(id, "error") then
  en_log("Found error in entry " .. id)
end
