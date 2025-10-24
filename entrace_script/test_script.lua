local r_start, r_end = en_span_range()
print("range", r_start, "..", r_end)
print("len", en_span_cnt())
filter_settings = {
	target = "meta.name",
	relation = "GT",
	value = "0",
}
filtered = en_filter_range(r_start, r_end, filter_settings)
print(filtered)
