--- @since 25.5.28
-- Yazi pixel-rendered Markdown previewer powered by `lume`

local M = {}

function M:peek(job)
	local cache = ya.file_cache(job)
	if not cache then
		return
	end

	local ok = self:preload(job)
	if not ok then
		return
	end

	ya.image_show(cache, job.area)
end

function M:seek(job)
	local h = cx.active.current.hovered
	if h and h.url == job.file.url then
		ya.emit("peek", { math.max(0, cx.active.preview.skip + job.units), only_if = job.file.url })
	end
end

function M:preload(job)
	local cache = ya.file_cache(job)
	if not cache or fs.cha(cache) then
		return true
	end

	local status, err = Command("lume")
		:arg({
			"--render-png",
			tostring(cache),
			"--pixel-width",
			"960",
			"--max-height",
			"1600",
			tostring(job.file.path),
		})
		:status()

	if not status or not status.success then
		return false, Err("lume failed: %s", err or (status and status.code) or "unknown")
	end
	return true
end

return M
