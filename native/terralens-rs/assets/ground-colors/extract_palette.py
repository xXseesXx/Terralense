#!/usr/bin/env python3
"""Extract average ground-block texture colors from a Prism TFG profile.

Usage: python3 assets/ground-colors/extract_palette.py /path/to/instance/minecraft
Requires Pillow. PNG input remains in the instance; only small RGB values are bundled.
"""
import json
import sys
import zipfile
from pathlib import Path
from PIL import Image
from io import BytesIO

ROOT = Path(sys.argv[1]).expanduser()
OUT = Path(__file__).with_name("palette.json")
LITTER_OUT = Path(__file__).with_name("litter_palette.json")
KINDS = {"rock/raw", "rock/gravel", "rock/cobble", "rock/mossy_cobble", "rock/magma", "sand", "dirt", "coarse_dirt", "clay", "mud", "rooted_dirt", "farmland"}

def rgb_mean(data):
    im = Image.open(BytesIO(data)).convert("RGBA")
    # Xaero World Map 1.47 samples a regular grid across the square texture area
    # `min(width, height)`, stepping by max(1, min(4, side // 8)). It averages
    # RGB equally for samples whose decoded alpha is > 10; alpha is not a weight.
    # Match those semantics instead of averaging every source texel.
    side = min(im.width, im.height)
    stride = max(1, min(4, side // 8))
    sample_count = side // stride
    samples = [im.getpixel((x * stride, y * stride)) for y in range(sample_count)
               for x in range(sample_count)]
    opaque = [p for p in samples if p[3] > 10]
    if not opaque:
        return [96, 96, 96]
    n = len(opaque)
    return [sum(p[c] for p in opaque) // n for c in range(3)]

def rgb_mean_alpha(data):
    im = Image.open(BytesIO(data)).convert("RGBA")
    side = min(im.width, im.height)
    stride = max(1, min(4, side // 8))
    count = side // stride
    samples = [im.getpixel((x * stride, y * stride)) for y in range(count)
               for x in range(count)]
    opaque = [p for p in samples if p[3] > 10]
    if not opaque:
        return {"rgb": [96, 96, 96], "opacity": 0}
    n = len(opaque)
    return {"rgb": [sum(p[c] for p in opaque) // n for c in range(3)],
            "opacity": sum(p[3] for p in opaque) // len(samples)}

def wanted(path):
    # namespace is supplied by the source root assets/<namespace>/textures/block/...
    p = path.split("/textures/block/", 1)
    if len(p) != 2:
        return None
    namespace, rel = p[0].rsplit("/", 1)[-1], p[1][:-4]
    if rel == "grass_top":
        # TFC's UP face uses this alpha overlay over the soil texture.
        return f"{namespace}:grass_tint_overlay", "tint_overlay"
    if rel.startswith("grass_path/"):
        return (f"{namespace}:{rel[:-4]}", "grass_path") if rel.endswith("_top") else None
    if rel.startswith("sandstone/bottom/"):
        # The shipped raw_sandstone models use the bottom texture on every face.
        return f"{namespace}:raw_sandstone/{rel.rsplit('/', 1)[1]}", "raw_sandstone"
    kind = "/".join(rel.split("/")[:2]) if rel.startswith("rock/") else rel.split("/")[0]
    if kind not in KINDS:
        return None
    # Column-model variants (notably podzol) use their *_top texture on the
    # exposed face. Rooted mud's top belongs to the muddy_roots registry family.
    if rel.startswith("mud/") and rel.endswith("_roots_top"):
        rel = "muddy_roots/" + rel.split("/", 1)[1][:-10]
    elif rel.endswith("_roots_side"):
        return None
    elif rel.endswith("_top"):
        rel = rel[:-4]
    return f"{namespace}:{rel}", kind

colors = {}
sources = {}
tree_colors = {}
litter_colors = {}
def add(path, data, override=False):
    texture_path = path.split("/textures/block/", 1)
    if len(texture_path) == 2:
        namespace = texture_path[0].rsplit("/", 1)[-1]
        rel = texture_path[1][:-4]
        for folder, color_name in (("wood/leaves/", "leaves"),
                                   ("wood/log_top/", "log_end"),
                                   ("wood/stripped_log_top/", "stripped_log_end"),
                                   ("wood/branch/", "branch")):
            if rel.startswith(folder):
                species = rel[len(folder):]
                if "/" not in species:
                    tree_colors.setdefault(f"{namespace}:{species}", {})[color_name] = rgb_mean(data)
                break
        if namespace == "tfg" and rel.startswith("palm_tree/"):
            if rel.endswith("_leaves_top"):
                species = rel.removeprefix("palm_tree/").removesuffix("_leaves_top")
                tree_colors.setdefault(f"tfg:palm_tree/{species}", {})["leaves"] = rgb_mean(data)
            elif rel.startswith("palm_tree/trunk_top_"):
                variant = rel.removeprefix("palm_tree/trunk_top_")
                tree_colors.setdefault(f"tfg:palm_tree/trunk_{variant}", {})["log_end"] = rgb_mean(data)
        if rel.startswith("wood/leaves/") and "/" not in rel[len("wood/leaves/"):]:
            species = rel.removeprefix("wood/leaves/")
            litter_colors[f"{namespace}:wood/fallen_leaves/{species}"] = rgb_mean_alpha(data)

    match = wanted(path)
    if not match and path.endswith(("/textures/block/halite.png", "/textures/block/hardened_clay.png", "/textures/block/tuff_gravel.png")) and "tfg/textures" in path:
        match = ("tfg:" + path.rsplit("/", 1)[1][:-4], "terrain")
    if not match and path.endswith("/textures/block/volcanic_ash.png"):
        match = ("tfg:pile/volcanic_ash", "volcanic_ash")
    if not match and path.endswith("/textures/block/duff_top.png"):
        for soil in ("alfisol", "loam", "mollisol", "oxisol", "podzol", "sandy_loam", "silt", "silty_loam"):
            match = (f"tfg:duff/{soil}", "duff")
            colors[match[0]] = rgb_mean(data)
        return
    vanilla = {
        "stone": "stone", "deepslate": "deepslate_top", "dirt": "dirt",
        "coarse_dirt": "coarse_dirt", "podzol": "podzol_top", "mycelium": "mycelium_top",
        "grass_block": "grass_block_top", "farmland": "farmland", "dirt_path": "dirt_path_top", "sand": "sand", "red_sand": "red_sand",
        "gravel": "gravel", "snow_block": "snow", "ice": "ice", "packed_ice": "packed_ice",
        "blue_ice": "blue_ice", "netherrack": "netherrack", "basalt": "basalt_top",
        "blackstone": "blackstone", "end_stone": "end_stone", "tuff": "tuff",
    }
    if not match and "/minecraft/textures/block/" in path:
        texture = path.rsplit("/textures/block/",1)[1][:-4]
        for block, tex in vanilla.items():
            if texture == tex:
                match = (f"minecraft:{block}", "vanilla")
                break
    if not match:
        # Surface clutter is kept in a separate tiny palette so loading it stays
        # optional and does not change ground-material fallback behavior.
        texture_path = path.split("/textures/block/", 1)
        if len(texture_path) == 2:
            namespace = texture_path[0].rsplit("/", 1)[-1]
            rel = texture_path[1][:-4]
            if (rel.startswith("groundcover/") or rel.startswith("wood/fallen_leaves/")
                    or rel.startswith("plant/dry_grass/") or rel.startswith("rock/loose/")):
                block = f"{namespace}:{rel}"
                if rel.startswith("plant/dry_grass/"):
                    block = f"{namespace}:plant/dry_grass"
                elif rel.startswith("rock/loose/"):
                    # Loose rock block models use the matching raw-rock texture.
                    block = f"{namespace}:rock/loose/{rel.rsplit('/', 1)[1]}"
                litter_colors[block] = rgb_mean_alpha(data)
        return
    key, kind = match
    if key in colors and not override and not path.endswith("_top.png"):
        return
    colors[key] = rgb_mean(data)
    sources[key] = path

for jar in sorted((ROOT / "mods").glob("*.jar")):
    try:
        with zipfile.ZipFile(jar) as z:
            for name in z.namelist():
                if name.endswith(".png") and "/textures/block/" in name:
                    add(name, z.read(name))
    except zipfile.BadZipFile:
        pass
for path in sorted((ROOT / "kubejs/assets").glob("*/textures/block/**/*.png")):
    add(path.as_posix(), path.read_bytes(), override=True)
MC = ROOT.parents[2] / "libraries/com/mojang/minecraft/1.20.1/minecraft-1.20.1-client.jar"
if MC.exists():
    with zipfile.ZipFile(MC) as z:
        for name in z.namelist():
            if name.startswith("assets/minecraft/textures/block/") and name.endswith(".png"):
                add(name, z.read(name))

# Reapply selected user texture packs in the order stored by Minecraft. Packs are
# layered over mod/base assets, and later selected packs have higher priority.
options = ROOT / "options.txt"
if options.exists():
    for line in options.read_text(encoding="utf-8").splitlines():
        if not line.startswith("resourcePacks:"):
            continue
        selected = json.loads(line.split(":", 1)[1])
        for pack_id in selected:
            if not pack_id.startswith("file/"):
                continue  # Built-in, mod-generated, or dynamic packs are not files here.
            pack = ROOT / "resourcepacks" / pack_id.removeprefix("file/")
            if pack.is_dir():
                for texture in pack.glob("assets/*/textures/block/**/*.png"):
                    add(texture.as_posix(), texture.read_bytes(), override=True)
            elif pack.is_file() and zipfile.is_zipfile(pack):
                with zipfile.ZipFile(pack) as z:
                    for name in z.namelist():
                        if "/textures/block/" in name and name.endswith(".png"):
                            add(name, z.read(name), override=True)

OUT.write_text(json.dumps(colors, indent=2, sort_keys=True) + "\n")
print(f"wrote {len(colors)} texture colors to {OUT}")
LITTER_OUT.write_text(json.dumps(litter_colors, indent=2, sort_keys=True) + "\n")
print(f"wrote {len(litter_colors)} litter texture colors to {LITTER_OUT}")
TREE_OUT = OUT.with_name("tree_palette.json")
# Preserve the feature aliases required by the runtime reader when refreshing
# texture colors alone. The separate tree extractor refreshes those aliases.
previous = json.loads(TREE_OUT.read_text()) if TREE_OUT.exists() else {}
if isinstance(previous.get("features"), dict):
    tree_output = {"schema": 1, "textures": tree_colors, "features": previous["features"]}
else:
    tree_output = tree_colors
TREE_OUT.write_text(json.dumps(tree_output, indent=2, sort_keys=True) + "\n")
print(f"wrote {len(tree_colors)} species colors to {TREE_OUT}")
