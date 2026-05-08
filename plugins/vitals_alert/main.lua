-- vitals_alert
--
-- Subscribes to Char.Vitals GMCP and echoes a colored warning when the
-- player's HP fraction drops below a threshold. Tracks the last warning
-- so the message only fires on the *transition* below the threshold,
-- not on every push afterward.

local THRESHOLD = 0.5
local last_was_low = false

local RED = "\27[1;31m"
local RESET = "\27[0m"

mud.on_gmcp("Char.Vitals", function(data)
  local hp = tonumber(data.hp)
  local maxhp = tonumber(data.maxhp)
  if not hp or not maxhp or maxhp <= 0 then
    return
  end
  local fraction = hp / maxhp
  if fraction < THRESHOLD and not last_was_low then
    mud.echo(RED .. "[vitals_alert] HP at " .. hp .. "/" .. maxhp .. RESET)
    last_was_low = true
  elseif fraction >= THRESHOLD then
    last_was_low = false
  end
end)
