-- SensibleFox installer progress window.
-- Polls /tmp/sensiblefox-install.status which is updated by postinstall.
-- Status file format (key=value, one per line):
--   step=init|download|mount|copy|configure|done|error
--   title=<string>
--   detail=<string>
--   progress=<int>     -- completed steps (0..total), -1 = indeterminate
--   total=<int>        -- total steps, -1 = indeterminate

property statusFile : "/tmp/sensiblefox-install.status"
property pollDelay : 0.2

on run
	set progress description to "SensibleFox"
	set progress additional description to "Preparing installation…"
	set progress total steps to -1
	set progress completed steps to 0
	
	set lastStep to ""
	set idleTicks to 0
	
	repeat
		set rec to readStatus()
		if rec is missing value then
			set idleTicks to idleTicks + 1
			if idleTicks > 150 then exit repeat
			delay pollDelay
		else
			set idleTicks to 0
			set currentStep to lookup(rec, "step", "")
			set currentTitle to lookup(rec, "title", "SensibleFox")
			set currentDetail to lookup(rec, "detail", "")
			set totalSteps to (lookup(rec, "total", "-1")) as integer
			set doneSteps to (lookup(rec, "progress", "0")) as integer
			
			set progress description to currentTitle
			set progress additional description to currentDetail
			if totalSteps ≤ 0 then
				set progress total steps to -1
			else
				set progress total steps to totalSteps
				if doneSteps < 0 then set doneSteps to 0
				if doneSteps > totalSteps then set doneSteps to totalSteps
				set progress completed steps to doneSteps
			end if
			
			if currentStep is "done" then
				delay 0.6
				exit repeat
			else if currentStep is "error" then
				display alert "SensibleFox install failed" message currentDetail as critical
				exit repeat
			end if
			set lastStep to currentStep
			delay pollDelay
		end if
	end repeat
end run

on readStatus()
	try
		set raw to do shell script "/bin/cat " & quoted form of statusFile & " 2>/dev/null"
	on error
		return missing value
	end try
	if raw is "" then return missing value
	set AppleScript's text item delimiters to {linefeed, return}
	set ls to text items of raw
	set AppleScript's text item delimiters to ""
	return ls
end readStatus

on lookup(ls, key, defaultValue)
	set prefix to key & "="
	set pl to count of prefix
	repeat with ln in ls
		set s to ln as string
		if (count of s) ≥ pl then
			if text 1 thru pl of s is prefix then
				return text (pl + 1) thru -1 of s
			end if
		end if
	end repeat
	return defaultValue
end lookup
