"""Builds the Yukari Gap content mod: one block, the gap, which the native mod draws facing the player.

usage: gen_yukari_gap.py <eyegap.bbmodel> <shard icon png> <content mod folder>
"""
import base64
import json
import shutil
import sys
from pathlib import Path

bbmodel, icon, out = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
NS, GAP, VERSION = "yukari_gap", "yukari_gap", "0.4.0"
if out.exists():
    shutil.rmtree(out)
oa, od = out / "assets" / NS, out / "data" / NS


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def write_bytes(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)


# The picture drawn in the world: the texture inside eyegap.bbmodel.
source = json.loads(bbmodel.read_text(encoding="utf-8"))["textures"][0]["source"]
write_bytes(oa / "textures" / "entity" / "gap.png", base64.b64decode(source.split(",", 1)[1]))
# The item: the shard.
write_bytes(oa / "textures" / "item" / f"{GAP}.png", icon.read_bytes())

# The block: glass underneath (it hides nothing beside it), an empty model (the native mod draws
# the gap), and nothing to bump into, so you can walk into it. It is two blocks, a lower and an
# upper half, so the gap can be clicked up where its picture is.
write(od / "horizon" / "block" / f"{GAP}.json", {
    "template": "minecraft:glass",
    "properties": {"hardness": 5.0, "explosion_resistance": 2000.0, "requires_tool": True,
                   "sounds": "minecraft:amethyst_block", "light": 7},
    "state_properties": {"half": {"type": "enum", "values": ["lower", "upper"]}},
    "default_state": {"half": "lower"},
    "shapes": {"default": [[0, 0, 0, 1, 1, 1]]},
    "collision_shapes": {"default": []},
    "item": True,
})
write(oa / "blockstates" / f"{GAP}.json",
      {"variants": {f"half={h}": {"model": f"{NS}:block/{GAP}"} for h in ("lower", "upper")}})
write(oa / "models" / "block" / f"{GAP}.json", {"textures": {"particle": f"{NS}:item/{GAP}"}, "elements": []})
write(oa / "items" / f"{GAP}.json", {"model": {"type": "minecraft:model", "model": f"{NS}:item/{GAP}"}})
write(oa / "models" / "item" / f"{GAP}.json",
      {"parent": "minecraft:item/generated", "textures": {"layer0": f"{NS}:item/{GAP}"}})
write(od / "loot_table" / "blocks" / f"{GAP}.json", {
    "type": "minecraft:block",
    # Only the lower half drops the gap.
    "pools": [{"rolls": 1, "conditions": [
        {"condition": "minecraft:block_state_property", "block": f"{NS}:{GAP}", "properties": {"half": "lower"}},
        {"condition": "minecraft:survives_explosion"}],
               "entries": [{"type": "minecraft:item", "name": f"{NS}:{GAP}"}]}],
})
# Amethyst shards, ender pearls and a gold ingot.
write(od / "recipe" / f"{GAP}.json", {
    "type": "minecraft:crafting_shaped",
    "key": {"A": "minecraft:amethyst_shard", "E": "minecraft:ender_pearl", "G": "minecraft:gold_ingot"},
    "pattern": ["AEA", "EGE", "AEA"],
    "result": {"id": f"{NS}:{GAP}"},
})
write(out / "data" / "minecraft" / "tags" / "block" / "mineable" / "pickaxe.json", {"values": [f"{NS}:{GAP}"]})
write(oa / "lang" / "en_us.json", {f"block.{NS}.{GAP}": "Yukari Gap", f"itemGroup.{NS}.{GAP}": "Yukari Gaps"})
write(od / "horizon" / "creative_tab" / f"{GAP}.json",
      {"title": "Yukari Gaps", "icon": f"{NS}:{GAP}", "items": [f"{NS}:{GAP}"]})
(out / "horizon-mod.toml").write_text(f'''id = "{NS}"
name = "Yukari Gaps Beta (assets)"
version = "{VERSION}"
description = "The blocks, item, art and recipe of Yukari Gaps Beta: gaps to travel between. Needs Yukari Gaps Beta (yukari_gap_native)."
authors = ["mxpbxb"]
api = "0.2"
side = "both"
capabilities = ["registries"]
''', encoding="utf-8")
print("built", out)
