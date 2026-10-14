-- The link between Lightroom and the Retoucan app: a shared folder.
--   bridge/state.json   written here every ~2s: catalog path + selection
--   bridge/jobs/*.lua   written by Retoucan: edits to apply
--   bridge/done/*.json  written here once a job is applied

local LrApplication = import 'LrApplication'
local LrFileUtils = import 'LrFileUtils'
local LrPathUtils = import 'LrPathUtils'
local LrTasks = import 'LrTasks'
local LrDialogs = import 'LrDialogs'
local LrProgressScope = import 'LrProgressScope'

local Bridge = { stop = false, busy = false }

function Bridge.config()
  local text = LrFileUtils.readFile(LrPathUtils.child(_PLUGIN.path, 'Config.lua'))
  local chunk = text and loadstring(text)
  return chunk and chunk() or {}
end

local function jsonString(s)
  s = tostring(s):gsub('\\', '\\\\'):gsub('"', '\\"'):gsub('\n', '\\n'):gsub('\r', '\\r'):gsub('\t', '\\t')
  return '"' .. s .. '"'
end

local function writeFile(path, text)
  local tmp = path .. '.tmp'
  local f = io.open(tmp, 'w')
  if not f then return false end
  f:write(text)
  f:close()
  if LrFileUtils.exists(path) then LrFileUtils.delete(path) end
  return LrFileUtils.move(tmp, path)
end

function Bridge.writeState(cfg)
  local catalog = LrApplication.activeCatalog()
  local parts = {}
  local ok, photos = pcall(function() return catalog:getTargetPhotos() end)
  if ok and photos then
    for i, photo in ipairs(photos) do
      if i > 5000 then break end
      local path = photo:getRawMetadata('path')
      if path then parts[#parts + 1] = jsonString(path) end
    end
  end
  local json = '{"time": ' .. os.time() .. ', "catalog": ' .. jsonString(catalog:getPath())
    .. ', "selected": [' .. table.concat(parts, ', ') .. ']}'
  writeFile(LrPathUtils.child(cfg.bridge, 'state.json'), json)
end

local function findPresets(uuids)
  local wanted, found = {}, {}
  for _, u in ipairs(uuids) do wanted[u] = true end
  for _, folder in ipairs(LrApplication.developPresetFolders()) do
    for _, preset in ipairs(folder:getDevelopPresets()) do
      local u = preset:getUuid()
      if wanted[u] then found[u] = preset end
    end
  end
  return found
end

local function applyJob(cfg, jobPath)
  local text = LrFileUtils.readFile(jobPath)
  local chunk, err = loadstring(text or '')
  if not chunk then return { applied = 0, skipped = 0, message = 'Unreadable job: ' .. tostring(err) } end
  local job = chunk()
  local catalog = LrApplication.activeCatalog()

  local wantedUuids = {}
  if job.look then wantedUuids[#wantedUuids + 1] = job.look.uuid end
  for _, u in ipairs(job.retouch or {}) do wantedUuids[#wantedUuids + 1] = u end
  local presets = findPresets(wantedUuids)
  local lookPreset = job.look and presets[job.look.uuid]
  local notes = {}
  if job.look and not lookPreset then
    notes[#notes + 1] = 'Look "' .. tostring(job.look.name) .. '" was not found in Lightroom.'
  end
  local missingRetouch = 0
  for _, u in ipairs(job.retouch or {}) do
    if not presets[u] then missingRetouch = missingRetouch + 1 end
  end
  if missingRetouch > 0 then
    notes[#notes + 1] = 'Retouch presets are installed but Lightroom needs a restart to load them.'
  end

  local applied, skipped = 0, 0
  local progress = LrProgressScope { title = 'Retoucan: editing ' .. #job.photos .. ' photos' }
  catalog:withWriteAccessDo('Retoucan edits', function()
    for i, item in ipairs(job.photos) do
      if progress:isCanceled() then break end
      local photo = catalog:findPhotoByPath(item.path)
      if photo then
        if lookPreset then photo:applyDevelopPreset(lookPreset, _PLUGIN) end
        photo:applyDevelopSettings(item.settings, 'Retoucan')
        if lookPreset and job.look.amount < 0.999 then
          -- Match Lightroom's preset Amount slider for the look's color table.
          pcall(function()
            local s = photo:getDevelopSettings()
            if s.Look then
              s.Look.Amount = job.look.amount
              photo:applyDevelopSettings({ Look = s.Look }, 'Retoucan look amount')
            end
          end)
        end
        for _, u in ipairs(job.retouch or {}) do
          if presets[u] then photo:applyDevelopPreset(presets[u], _PLUGIN) end
        end
        applied = applied + 1
      else
        skipped = skipped + 1
      end
      progress:setPortionComplete(i, #job.photos)
    end
  end, { timeout = 30 })
  progress:done()
  if skipped > 0 then notes[#notes + 1] = skipped .. ' photos are not in this catalog.' end
  return { applied = applied, skipped = skipped, message = table.concat(notes, ' ') }
end

-- Apply every waiting job. Returns how many photos were edited.
function Bridge.processJobs(cfg)
  -- The watcher and the menu command can both call this; never apply a job twice.
  if Bridge.busy then return 0 end
  Bridge.busy = true
  local ok, total = LrTasks.pcall(Bridge.processJobsUnlocked, cfg)
  Bridge.busy = false
  return ok and total or 0
end

function Bridge.processJobsUnlocked(cfg)
  local jobsDir = LrPathUtils.child(cfg.bridge, 'jobs')
  local doneDir = LrPathUtils.child(cfg.bridge, 'done')
  if not LrFileUtils.exists(jobsDir) then return 0 end
  local jobs = {}
  for path in LrFileUtils.files(jobsDir) do
    if LrPathUtils.extension(path) == 'lua' then jobs[#jobs + 1] = path end
  end
  table.sort(jobs)
  local total = 0
  for _, path in ipairs(jobs) do
    local ok, result = LrTasks.pcall(applyJob, cfg, path)
    if not ok then result = { applied = 0, skipped = 0, message = 'Error: ' .. tostring(result) } end
    total = total + (result.applied or 0)
    local id = LrPathUtils.removeExtension(LrPathUtils.leafName(path))
    LrFileUtils.createAllDirectories(doneDir)
    writeFile(LrPathUtils.child(doneDir, id .. '.json'),
      '{"applied": ' .. (result.applied or 0) .. ', "skipped": ' .. (result.skipped or 0)
      .. ', "message": ' .. jsonString(result.message or '') .. '}')
    LrFileUtils.delete(path)
  end
  return total
end

-- Background loop started when Lightroom loads the plugin.
function Bridge.watch()
  LrTasks.startAsyncTask(function()
    local cfg = Bridge.config()
    if not cfg.bridge then return end
    LrFileUtils.createAllDirectories(LrPathUtils.child(cfg.bridge, 'jobs'))
    local tick = 0
    while not Bridge.stop do
      LrTasks.pcall(Bridge.processJobs, cfg)
      if tick % 2 == 0 then LrTasks.pcall(Bridge.writeState, cfg) end
      tick = tick + 1
      LrTasks.sleep(1)
    end
  end)
end

function Bridge.runApp(cfg, args)
  local cmd = '"' .. cfg.app .. '" ' .. args
  if WIN_ENV then cmd = '"' .. cmd .. '"' end
  return LrTasks.execute(cmd)
end

return Bridge
