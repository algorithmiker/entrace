local fs = en_filterset_from_range(en_span_range())
en_filterset_materialize(en_filter("message", "EQ", "constructed node", fs))
