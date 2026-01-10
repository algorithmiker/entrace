Get metadata of a span.

## INPUT
A span id.

## OUTPUT
A table:
- name: string
- level: int (1=Trace, 2=Debug, 3=Info, 4=Warn, 5=Error)
- file: string or nil
- line: int or nil
- target: string
- module_path: string or nil

## EXAMPLE
local meta = en_metadata_table(id)
en_log(meta.name)