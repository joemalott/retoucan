-- Send the selected photos to Retoucan. It predicts the edits and queues a
-- job, which the background watcher applies a moment later.
local LrTasks = import 'LrTasks'
local LrDialogs = import 'LrDialogs'
local Bridge = require 'Bridge'

LrTasks.startAsyncTask(function()
  local cfg = Bridge.config()
  Bridge.writeState(cfg)
  local rc = Bridge.runApp(cfg, 'edit-selection')
  if rc ~= 0 then
    LrDialogs.message('Retoucan could not edit these photos',
      'Open the Retoucan app to check that a style is chosen, then try again.', 'critical')
    return
  end
  Bridge.processJobs(cfg)
end)
