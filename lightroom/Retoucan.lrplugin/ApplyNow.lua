local LrTasks = import 'LrTasks'
local LrDialogs = import 'LrDialogs'
local Bridge = require 'Bridge'

LrTasks.startAsyncTask(function()
  local n = Bridge.processJobs(Bridge.config())
  LrDialogs.showBezel(n > 0 and ('Retoucan edited ' .. n .. ' photos') or 'No Retoucan edits waiting')
end)
