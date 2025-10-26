function query1()
	local r_start, r_end = en_span_range()
	local ids = {}
	for i = r_start, r_end do
		if en_child_cnt(i) > 2 then
			table.insert(ids, i)
		end
	end
	return ids
end

function query4()
	local r_start, r_end = en_span_range()
	local ids = {}
	for i = r_start, r_end do
		if not en_contains_anywhere(i, "winit") then
			table.insert(ids, i)
		end
	end
	return ids
end

function query5()
	local r_start, r_end = en_span_range()
	local ids = {}
	for i = r_start, r_end do
		local msg_cnt = en_attr_by_name(i, "msg_idx")
		if msg_cnt ~= nil and math.fmod(msg_cnt, 2) == 1 then
			table.insert(ids, i)
		end
	end
	return ids
end

function query6()
	local r_start, r_end = en_span_range()
	local ids = {}
	for i = r_start, r_end do
		local is_winit = en_metadata_target(i) == "winit::window"
		local log_module_path = en_attr_by_name(i, "log.module_path")
		local is_eframe_related = log_module_path ~= nil and string.match(log_module_path, "eframe")
		if is_winit or is_eframe_related then
			table.insert(ids, i)
		end
	end
	return ids
end

function query7()
	local r_start, r_end = en_span_range()
	local ids = {}
	for i = r_start, r_end do
		local node_value = en_attr_by_name(i, "node_value")
		if node_value and node_value >= "d3" then
			table.insert(ids, i)
		end
	end
	return ids
end

-- equivalent to query7(), but about 2x faster.
function query8()
	local r_start, r_end = en_span_range()

	filter_settings = {
		target = "node_value",
		relation = "GT",
		value = "d3",
	}
	filtered = en_filter_range(r_start, r_end, filter_settings)
	return filtered
end

-- we are looking for the spans where
-- message = "constructed node"
-- breadth > 1

-- This is the "old fashioned" no filterset implementation
function query9()
	local rstart, rend = en_span_range()
	local ids = {}
	for i = rstart, rend do
		local message_matches = en_attr_by_name(i, "message") == "constructed node"
		if message_matches and (en_attr_by_name(i, "breadth") > 1) then
			table.insert(ids, i)
		end
	end
	return ids
end

function query10()
	local rstart, rend = en_span_range()
	local base = en_filterset_from_range(rstart, rend)

	msg_filter_desc = { target = "message", value = "constructed node", relation = "EQ" }
	local message_matches = en_filter(msg_filter_desc, base)

	breadth_filter_desc = { target = "breadth", value = 1, relation = "GT" }
	local breadth_matches = en_filter(breadth_filter_desc, message_matches)

	final = breadth_matches

	materialized = en_filterset_materialize(final)
	return materialized
end
