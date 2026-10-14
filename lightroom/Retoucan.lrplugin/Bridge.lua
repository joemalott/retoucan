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

local Bridge = { busy = false }

-- Written to Lightroom's log folder (LrClassicLogs/Retoucan.log).
local log = (import 'LrLogger')('Retoucan')
log:enable('logfile')
Bridge.log = log

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
  local ok, photos = LrTasks.pcall(function() return catalog:getTargetPhotos() end)
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

local function trim(s)
  return (tostring(s or ''):gsub('^%s+', ''):gsub('%s+$', ''):lower())
end

-- Find presets by UUID, falling back to group + name. Lightroom gives
-- presets its own UUIDs, which don't match the crs:UUID in the .xmp file.
-- Each wanted entry is { uuid, group, name }; results are keyed by uuid.
-- Lightroom may show "Retoucan - Smooth skin" as just "Smooth skin".
local function findPresets(cfg, wanted)
  local found, byUuid, seen = {}, {}, {}
  for _, w in ipairs(wanted) do byUuid[w.uuid] = w end
  for _, folder in ipairs(LrApplication.developPresetFolders()) do
    local group = trim(folder:getName())
    for _, preset in ipairs(folder:getDevelopPresets()) do
      local u, name = preset:getUuid(), trim(preset:getName())
      if byUuid[u] then found[u] = preset end
      for _, w in ipairs(wanted) do
        if not found[w.uuid] and group == trim(w.group) then
          local wn = trim(w.name)
          if name == wn or group .. ' - ' .. name == wn then found[w.uuid] = preset end
        end
      end
      seen[#seen + 1] = folder:getName() .. '\t' .. preset:getName() .. '\t' .. tostring(u)
    end
  end
  -- When something is missing, record what Lightroom does see.
  for _, w in ipairs(wanted) do
    if not found[w.uuid] then
      writeFile(LrPathUtils.child(cfg.bridge, 'presets-seen.txt'),
        'missing: ' .. tostring(w.group) .. ' / ' .. tostring(w.name) .. '\n' .. table.concat(seen, '\n') .. '\n')
      break
    end
  end
  return found
end

local SNAPSHOT = 'Before Retoucan'

local function findSnapshot(photo)
  local ok, snaps = LrTasks.pcall(function() return photo:getDevelopSnapshots() end)
  if not ok or not snaps then return nil end
  for _, s in ipairs(snaps) do
    if s.name == SNAPSHOT then return s end
  end
  return nil
end

local function findKeyword(catalog)
  for _, k in ipairs(catalog:getKeywords()) do
    if k:getName() == 'Retoucan' then return k end
  end
  return nil
end

-- Mask safety checks. After Lightroom's AI detection each Retoucan mask is
-- one of: found (has a MaskDigest), empty (ErrorReason ~= 0: nothing to find,
-- e.g. no faces or no sky in the photo) or pending (not computed yet).
local function maskStatus(photo)
  local groups = photo:getDevelopSettings().MaskGroupBasedCorrections or {}
  local empty, pending = {}, 0
  for i, g in ipairs(groups) do
    if tostring(g.CorrectionName or ''):find('^Retoucan ') then
      local masks = g.CorrectionMasks or {}
      local failed, waiting = 0, 0
      for _, m in ipairs(masks) do
        local err = tonumber(m.ErrorReason or 0) or 0
        if err ~= 0 then
          failed = failed + 1
        elseif not m.MaskDigest then
          waiting = waiting + 1
        end
      end
      if #masks > 0 and failed == #masks then
        empty[i] = true
      elseif waiting > 0 then
        pending = pending + 1
      end
    end
  end
  return groups, empty, pending
end

-- Detect masks on each photo, retry ones that didn't compute, and remove
-- masks that found nothing so Lightroom shows no warnings. Returns counts and
-- the photos that still need a look.
local function finishMasks(catalog, photos, onProgress)
  local removed, attention = 0, {}
  for i, photo in ipairs(photos) do
    LrTasks.pcall(function() photo:updateAISettings() end)
    local ok, groups, empty, pending = LrTasks.pcall(maskStatus, photo)
    if ok and pending > 0 then
      LrTasks.sleep(0.5)
      LrTasks.pcall(function() photo:updateAISettings() end)
      ok, groups, empty, pending = LrTasks.pcall(maskStatus, photo)
    end
    if ok and next(empty) then
      local keep = {}
      for j, g in ipairs(groups) do
        if empty[j] then removed = removed + 1 else keep[#keep + 1] = g end
      end
      LrTasks.pcall(function()
        catalog:withWriteAccessDo('Retoucan remove empty masks', function()
          photo:applyDevelopSettings({ MaskGroupBasedCorrections = keep }, 'Retoucan remove empty masks')
        end, { timeout = 30 })
      end)
    end
    if not ok or pending > 0 then attention[#attention + 1] = photo end
    if onProgress then onProgress(i, #photos) end
  end
  return removed, attention
end

local ATTENTION = 'Retoucan: check these'

local function maskNotes(notes, removed, attention)
  if removed > 0 then
    notes[#notes + 1] = 'Removed ' .. removed .. ' masks that had nothing to find (like skin with no people, or sky indoors).'
  end
  if #attention > 0 then
    notes[#notes + 1] = #attention .. ' photos still have masks Lightroom could not compute; they are in the "' .. ATTENTION .. '" collection.'
  end
end

-- undo: back to the "Before Retoucan" snapshot, then forget it.
-- cleanup: keep the edits, drop the snapshot.
-- applyDevelopSnapshot/deleteDevelopSnapshot do their own catalog writes and
-- are silently ignored inside withWriteAccessDo, so they run outside it.
local function applyAction(job)
  local catalog = LrApplication.activeCatalog()
  if job.action == 'check' then
    local photos = {}
    for _, path in ipairs(job.photos) do
      local photo = catalog:findPhotoByPath(path)
      if photo then photos[#photos + 1] = photo end
    end
    local progress = LrProgressScope { title = 'Retoucan: checking masks on ' .. #photos .. ' photos' }
    local removed, attention = finishMasks(catalog, photos, function(i, n) progress:setPortionComplete(i, n) end)
    progress:done()
    if #attention > 0 then LrTasks.pcall(showInCollection, catalog, ATTENTION, attention) end
    local notes = {}
    maskNotes(notes, removed, attention)
    Bridge.log:info('check: ' .. #photos .. ' photos, ' .. removed .. ' empty masks removed, ' .. #attention .. ' need attention')
    return { applied = #photos, skipped = #job.photos - #photos, message = table.concat(notes, ' ') }
  end
  local keyword = findKeyword(catalog)
  local done, skipped = 0, 0
  local title = job.action == 'undo' and 'Retoucan: undoing ' or 'Retoucan: cleaning up '
  local progress = LrProgressScope { title = title .. #job.photos .. ' photos' }
  local undone = {}
  for i, path in ipairs(job.photos) do
    if progress:isCanceled() then break end
    local photo = catalog:findPhotoByPath(path)
    local snap = photo and findSnapshot(photo)
    if snap and snap.snapshotID then
      if job.action == 'undo' then
        photo:applyDevelopSnapshot(snap.snapshotID)
        undone[#undone + 1] = photo
      end
      photo:deleteDevelopSnapshot(snap.snapshotID)
      done = done + 1
    else
      skipped = skipped + 1
    end
    progress:setPortionComplete(i, #job.photos)
  end
  if #undone > 0 then
    catalog:withWriteAccessDo('Retoucan undo', function()
      for _, photo in ipairs(undone) do
        if keyword then photo:removeKeyword(keyword) end
        -- Applying a snapshot merges settings, so masks it doesn't mention
        -- survive. Drop Retoucan's own masks; keep any the user added.
        local groups = photo:getDevelopSettings().MaskGroupBasedCorrections or {}
        local keep = {}
        for _, g in ipairs(groups) do
          if not tostring(g.CorrectionName or ''):find('^Retoucan ') then keep[#keep + 1] = g end
        end
        if #keep < #groups then
          photo:applyDevelopSettings({ MaskGroupBasedCorrections = keep }, 'Retoucan undo masks')
        end
      end
    end, { timeout = 60 })
  end
  progress:done()
  local verb = job.action == 'undo' and 'undone' or 'cleaned up'
  local message = skipped > 0 and (skipped .. ' photos had no Retoucan snapshot to ' .. job.action .. '.') or ''
  Bridge.log:info(job.action .. ': ' .. done .. ' ' .. verb .. ', ' .. skipped .. ' skipped')
  return { applied = done, skipped = skipped, message = message }
end

-- Put a job's photos in a collection (replacing what was there) and show it.
local function showInCollection(catalog, name, photos)
  local collection
  catalog:withWriteAccessDo('Retoucan collection', function()
    collection = catalog:createCollection(name, nil, true)
    if collection then
      collection:removeAllPhotos()
      collection:addPhotos(photos)
    end
  end, { timeout = 30 })
  if collection then LrTasks.pcall(function() catalog:setActiveSources { collection } end) end
end

local function applyJob(cfg, jobPath)
  local text = LrFileUtils.readFile(jobPath)
  local chunk, err = loadstring(text or '')
  if not chunk then return { applied = 0, skipped = 0, message = 'Unreadable job: ' .. tostring(err) } end
  local job = chunk()
  if job.action then return applyAction(job) end
  local catalog = LrApplication.activeCatalog()

  -- Older jobs list retouch presets as bare UUIDs.
  local retouch = {}
  for _, r in ipairs(job.retouch or {}) do
    retouch[#retouch + 1] = type(r) == 'table' and r or { uuid = r }
  end
  local wanted = {}
  if job.look then wanted[#wanted + 1] = job.look end
  for _, r in ipairs(retouch) do wanted[#wanted + 1] = r end
  local presets = findPresets(cfg, wanted)
  local lookPreset = job.look and presets[job.look.uuid]
  local notes = {}
  if job.look and not lookPreset then
    notes[#notes + 1] = 'Look "' .. tostring(job.look.name) .. '" was not found in Lightroom.'
  end
  local missingRetouch = 0
  for _, r in ipairs(retouch) do
    if not presets[r.uuid] then missingRetouch = missingRetouch + 1 end
  end
  if missingRetouch > 0 then
    notes[#notes + 1] = missingRetouch .. ' retouch presets were not found in Lightroom. Restart Lightroom if they were just installed.'
  end

  local applied, skipped = 0, 0
  local edited = {}
  local total = #job.photos
  local progressPath = LrPathUtils.child(LrPathUtils.child(cfg.bridge, 'progress'), tostring(job.id) .. '.json')
  LrFileUtils.createAllDirectories(LrPathUtils.parent(progressPath))
  -- Retoucan shows this while the job runs.
  local function report(stage, done, count)
    writeFile(progressPath, '{"stage": ' .. jsonString(stage) .. ', "done": ' .. done .. ', "total": ' .. count .. '}')
  end

  local function editPhoto(photo, item, keyword)
    -- Keep the photo's pre-Retoucan state so it can be undone in one click.
    -- Only the first time: re-editing must not overwrite the true original.
    if not findSnapshot(photo) then
      local ok, e = LrTasks.pcall(function() photo:createDevelopSnapshot(SNAPSHOT, false) end)
      if not ok then Bridge.log:error('snapshot: ' .. tostring(e)) end
    end
    local before = photo:getDevelopSettings().Look
    if lookPreset then photo:applyDevelopPreset(lookPreset, _PLUGIN) end
    photo:applyDevelopSettings(item.settings, 'Retoucan')
    if lookPreset and job.look.amount < 0.999 then
      -- Presets that carry a profile look (color table) get the Amount;
      -- the sliders were already blended at that amount by Retoucan.
      LrTasks.pcall(function()
        local look = photo:getDevelopSettings().Look
        if look and (not before or look.Name ~= before.Name) then
          look.Amount = job.look.amount
          photo:applyDevelopSettings({ Look = look }, 'Retoucan look amount')
        end
      end)
    end
    for _, r in ipairs(retouch) do
      if presets[r.uuid] then photo:applyDevelopPreset(presets[r.uuid], _PLUGIN) end
    end
    if keyword then photo:addKeyword(keyword) end
  end

  -- Apply in batches so a long job doesn't hold one huge write transaction,
  -- and an error only loses its own batch.
  local progress = LrProgressScope { title = 'Retoucan: editing ' .. total .. ' photos' }
  local BATCH = 25
  local canceled = false
  for start = 1, total, BATCH do
    if progress:isCanceled() then canceled = true break end
    local ok, e = LrTasks.pcall(function()
      catalog:withWriteAccessDo('Retoucan edits', function()
        -- Tag edited photos so they can be found (and skipped) later.
        local keyword = catalog:createKeyword('Retoucan', {}, false, nil, true)
        for i = start, math.min(start + BATCH - 1, total) do
          local item = job.photos[i]
          local photo = catalog:findPhotoByPath(item.path)
          if photo then
            editPhoto(photo, item, keyword)
            applied = applied + 1
            edited[#edited + 1] = photo
          else
            skipped = skipped + 1
          end
        end
      end, { timeout = 60 })
    end)
    if not ok then
      Bridge.log:error('batch at ' .. start .. ': ' .. tostring(e))
      notes[#notes + 1] = 'Some photos failed: ' .. tostring(e)
    end
    local done = math.min(start + BATCH - 1, total)
    progress:setPortionComplete(done, total)
    report('Applying edits', done, total)
  end
  progress:done()

  -- Presets only store the AI mask definitions; Lightroom has to detect
  -- subject, skin, sky etc. on each photo. photo:updateAISettings() arrived
  -- in Lightroom 13.3. Without it the masks show a warning until the user
  -- runs Photo > Develop Settings > Update AI Settings.
  if #retouch > 0 and #edited > 0 and not canceled then
    if edited[1].updateAISettings then
      local aiProgress = LrProgressScope { title = 'Retoucan: detecting masks on ' .. #edited .. ' photos' }
      local removed, attention = finishMasks(catalog, edited, function(i, n)
        aiProgress:setPortionComplete(i, n)
        report('Detecting masks', i, n)
      end)
      aiProgress:done()
      if #attention > 0 then LrTasks.pcall(showInCollection, catalog, ATTENTION, attention) end
      maskNotes(notes, removed, attention)
    else
      notes[#notes + 1] = 'Run Photo > Develop Settings > Update AI Settings to finish the masks.'
    end
  end
  if job.collection and #edited > 0 then
    local ok, e = LrTasks.pcall(showInCollection, catalog, job.collection, edited)
    if not ok then Bridge.log:error('collection: ' .. tostring(e)) end
  end
  if canceled then notes[#notes + 1] = 'Canceled in Lightroom after ' .. applied .. ' photos.' end
  LrFileUtils.delete(progressPath)

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
  if not ok then log:error('processJobs: ' .. tostring(total)) end
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
  for _, queued in ipairs(jobs) do
    -- Claim the job first. A plugin reload can leave an old loop running
    -- next to the new one; only the loop whose rename lands runs the job.
    local path = LrPathUtils.replaceExtension(queued, 'running')
    LrFileUtils.move(queued, path)
    if LrFileUtils.exists(path) and not LrFileUtils.exists(queued) then
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
  end
  return total
end

-- Background loop started when Lightroom loads the plugin.
-- Exactly one watcher loop may run. A plugin reload starts a new loop in a
-- fresh copy of this module while the old loop keeps going, and shared
-- state between the two copies isn't reliable. So each loop writes its own
-- token to a file and exits as soon as the file holds someone else's.
function Bridge.watch()
  LrTasks.startAsyncTask(function()
    local cfg = Bridge.config()
    log:info('watcher starting, bridge = ' .. tostring(cfg.bridge))
    if not cfg.bridge then return end
    LrFileUtils.createAllDirectories(LrPathUtils.child(cfg.bridge, 'jobs'))
    local tokenPath = LrPathUtils.child(cfg.bridge, 'watcher.token')
    local token = tostring(os.time()) .. '-' .. tostring(math.random(1, 1e9))
    writeFile(tokenPath, token)
    local tick, lastError = 0, nil
    local function check(ok, e)
      if not ok and tostring(e) ~= lastError then
        lastError = tostring(e)
        log:error(lastError)
      end
    end
    while LrFileUtils.readFile(tokenPath) == token do
      check(LrTasks.pcall(Bridge.processJobs, cfg))
      if tick % 2 == 0 then check(LrTasks.pcall(Bridge.writeState, cfg)) end
      tick = tick + 1
      LrTasks.sleep(1)
    end
    log:info('watcher stopped (a newer one took over)')
  end)
end

function Bridge.runApp(cfg, args)
  local cmd = '"' .. cfg.app .. '" ' .. args
  if WIN_ENV then cmd = '"' .. cmd .. '"' end
  return LrTasks.execute(cmd)
end

return Bridge
