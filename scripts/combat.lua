-- combat.lua — Phase 8 demo combat assistant.
-- Place at ~/Library/Application Support/com.aabahran.mudclient/scripts/combat.lua
-- and run "#script load combat" in the input box.

mud.echo("combat assistant loaded")

-- Auto-loot when something dies in combat. Captures the slain mob name and
-- sends `get all corpse of <name>`.
mud.trigger("auto_loot", "(\\w+) is DEAD!", function(c)
  mud.send("get all corpse of " .. c[2])
  mud.echo("[auto-loot] " .. c[2])
end)

-- Flee when HP drops below 25 percent. Reads from the GMCP Char.Vitals
-- push, which Aabahran sends every round.
local fleeing = false
mud.on_gmcp("Char.Vitals", function(d)
  if not d.hp or not d.maxhp then return end
  local pct = (tonumber(d.hp) or 0) / (tonumber(d.maxhp) or 1)
  if pct < 0.25 and not fleeing then
    fleeing = true
    mud.echo("[low hp, fleeing]")
    mud.send("flee")
  elseif pct >= 0.5 then
    fleeing = false
  end
end)

-- Quick combat alias. `kk goblin` runs a kick + backstab pair.
mud.alias("kk", "kick %1; backstab %1")

-- Schedule a one-shot timer when a tell arrives so we remember to reply.
mud.trigger("tell_reminder", "(\\w+) tells you", function(c)
  mud.echo("[tell from " .. c[2] .. ", reminding in 30s]")
  mud.timer(30, function()
    mud.echo("[reminder] you have not replied to " .. c[2])
  end)
end)
