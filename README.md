# Yukari Gaps Beta

Gaps to travel between, for [Horizon](https://playhorizon.dev). Place a gap, name it, and step from
one gap to any other.

**Beta:** it works, but it is new. Report anything odd in this repository's issues.

## Two parts

Install both from the launcher's Mods page, then press Build:

- **Yukari Gaps Beta** (`yukari_gap_native`, the `.hnmod`): the code. It is a native mod: it runs
  with your account's permissions, so the launcher asks before building it.
- **Yukari Gaps Beta (assets)** (`yukari_gap`, the `.hmod`): the block, its item, its art and its recipe.

## How it plays

- **Craft** a Yukari Gap from four amethyst shards, four ender pearls and a gold ingot:

  ```
  A E A
  E G E
  A E A
  ```

- **Place** it where there are two free blocks. A window asks for its name: type one and press Enter
  or Done. Left empty, it keeps a made-up name. If another gap has the name already, yours gets a
  number: `Home (2)`, `Home (3)`.
- **Mine** it with anything, a pickaxe, another tool or your hand, and it drops as its item.
- **Lock** it in the same window and only you can use it, or break it. While it is locked you can
  **share** it there with players: those online, and anyone it is already shared with.
- **Find** a gap before you can travel to it: right-click it once. Your own gaps are found already.
- **Make it a Giga Gap** with a **Yukari Umbrella**: right-click your own gap with one and it becomes
  a Giga Gap, global: everyone can travel to it without finding it (still not while it is locked).
  Do it again to make it a Yukari Gap again. Each use
  takes the umbrella, in Creative too: each umbrella is good for one use. Umbrellas can't be
  crafted: Yukari (Touhou Mobs) drops one 70 times in 100 when she is beaten, and Yukari Great Doll
  every time. A Yukari who comes through a global gap is always the Great Doll. It is drawn as Touhou Items' parasol, so install that mod to see it.
- **Right-click** any gap for every other gap you may use (found, global, or shared with you): tiles in rows, a new row when one is full,
  and a scroll bar when there are more than fit. Hover over one for where it is; click it to go.
- **Travel is free**, however far, and to any dimension.
- **A Yukari Doll** ([Alice Dolls Restringed](https://github.com/mxpbxb/alice-dolls-restringed)) is
  her owner's portable gap: right-click her with an empty hand for the same screen, and she follows
  you through. It only goes one way: she is never on the list.
- **Sneak and right-click** your own gap to rename it, change its lock or who it is shared with.
- In the world the gap is pipef0x's Yukari Gap Demo look: an S-shaped gap that slowly breathes, full of
  drifting purple fog and wandering eyes, with a red bow swinging at each tip. It always turns to face you.
  You can walk into it. Click or break it up at the picture, in the block above the one you placed
  it on.
- A **Giga Gap** is drawn a quarter bigger, with another look: a dark back wall and red-eyed eyes
  drifting across it. It has no item of its own: mined, it drops the Yukari Gap it was made from.

The world keeps its gaps in `<world>/yukari_gap/gaps.tsv`.

## Building

`gen_yukari_gap.py art/gap.png art/shard.png content` writes the content mod from the art.
`bake_gap.py content/assets/yukari_gap/textures/entity/gap.png` then bakes the gap's picture over it:
pipef0x's gap shader, run in Python into 192 frames (48 seconds, looping) with its `.mcmeta`.
`bake_gap_alt.py art/gap_alt_back.png art/gap_alt_eye.png content/assets/yukari_gap/textures/entity/giga_gap.png`
bakes the Giga Gap's picture: the same breathing gap and drifting eyes, filled with the back wall
picture, each eye drawn as the eye picture. The back may be animated: draw its frames as one PNG,
square frames stacked top to bottom (64x64 each, so 64 wide and 64 x N tall), and bake again. The
gap shows the next back frame four times a second, so 8 frames make a 2-second pulse; for a clean
loop make N divide 192.
`shaders/gap.wgsl` is the demo's gap shader (`fog.vsh` and `fog.fsh`) translated from GLSL to WGSL,
for drawing the gap live instead of from baked frames once Horizon lets mods bring shaders.
`cargo horizon-mod build` in `content/` makes the `.hmod`; the native mod's source is `native/`, built
by the Horizon launcher (`native-build`).

## Credits

- Code and art by mxpbxb. MIT licensed: see `LICENSE`.
- The gap's look (its shader, baked into `textures/entity/gap.png`) and the bow (`textures/entity/bow.png`)
  are from [Yukari Gap Demo](https://github.com/pipef0x/Yukari-Gap-Demo) by pipef0x, MIT licensed,
  as is `shaders/gap.wgsl`, its shader in WGSL.
  The Giga Gap (`textures/entity/giga_gap.png`) keeps its shape and eye motion, with mxpbxb's back
  wall and eye (`art/gap_alt_back.png`, `art/gap_alt_eye.png`).
- The gap item's animated icon (`textures/item/yukari_gap.png`, `art/shard.png`) is Touhou Little Maid's wireless IO art
  by TartaricAcid and others, by way of Alice Dolls Restringed, under
  [CC BY-NC-SA 4.0](https://creativecommons.org/licenses/by-nc-sa/4.0/).
- Inspired by [Waystones](https://www.curseforge.com/minecraft/mc-mods/waystones) by BlayTheNinth.
  No code or assets of Waystones are in this mod.
- Yukari Yakumo and her gaps are from the Touhou Project by ZUN (Team Shanghai Alice). This is a
  non-commercial fan work.
- No Minecraft code or assets are included.
