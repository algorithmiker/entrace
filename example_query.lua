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
