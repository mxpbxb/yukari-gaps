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
- **Lock** it in the same window and only you can use it, or break it. Unlocked, it is everyone's.
- **Right-click** any gap for every other gap you may use: tiles in rows, a new row when one is full,
  and a scroll bar when there are more than fit. Hover over one for where it is; click it to go.
- **The cost** is a level for each full 500 blocks, at most three, and three to another dimension.
  Free in creative.
- **Sneak and right-click** your own gap to rename it or change its lock.
- In the world the gap is an eye-gap picture that leans 45 degrees and always turns to face you.
  You can walk into it.

The world keeps its gaps in `<world>/yukari_gap/gaps.tsv`.

## Building

`gen_yukari_gap.py art/eyegap.bbmodel art/shard.png content` writes the content mod from the art.
`cargo horizon-mod build` in `content/` makes the `.hmod`; the native mod's source is `native/`, built
by the Horizon launcher (`native-build`).

## Credits

- Code and art by mxpbxb. MIT licensed: see `LICENSE`.
- Inspired by [Waystones](https://www.curseforge.com/minecraft/mc-mods/waystones) by BlayTheNinth.
  No code or assets of Waystones are in this mod.
- Yukari Yakumo and her gaps are from the Touhou Project by ZUN (Team Shanghai Alice). This is a
  non-commercial fan work.
- No Minecraft code or assets are included.
