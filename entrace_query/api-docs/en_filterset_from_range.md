 en_filterset_from_range()
  input: start, end
  outputs: a table with
    type: "filterset"
    root: 0
    items: {
      { type = "prim_range"; start = start, end=end}
    }