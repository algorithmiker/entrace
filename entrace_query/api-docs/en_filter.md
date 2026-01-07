 en_filter()
 input:
   filter: table with
     target: name of variable eg. "message" or "meta.filename"
     relation: a relation, one of "EQ", "LT", "GT"
     value: a constant to compare with
   src: filterset
 outputs: { type = "filterset", root = 1, items = { src = 0, {type = "rel_dnf", src = 0, clauses = {{ target, relation, value}} }}},