local rstart, rend = en_span_range()
local base = en_filterset_from_range(rstart, rend)

msg_filter_desc = { target = "message", value = "constructed node", relation = "EQ" }
local message_matches = en_filter(msg_filter_desc, base)

breadth_filter_desc = { target = "breadth", value = 1, relation = "GT" }
local breadth_matches = en_filter(breadth_filter_desc, base)

both_matches = en_filterset_dnf({
	{
		{ target = "breadth", value = 1,                  relation = "GT" },
		{ target = "message", value = "constructed node", relation = "EQ" }
	}
}, base)
print(en_pretty_table(both_matches))
