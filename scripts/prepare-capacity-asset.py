#!/usr/bin/env python3
"""Prepare the pinned CC0 Water Bottle as a welded OBJ with original corner UVs."""
import argparse
import hashlib
import json
import struct
import urllib.request
from pathlib import Path

REVISION = "4995a638c96dbc94376d3d6164e70fee3ac7f7a2"
ROOT = f"https://raw.githubusercontent.com/KhronosGroup/glTF-Sample-Assets/{REVISION}/Models/WaterBottle/glTF/"
BLOBS = {
    "WaterBottle.gltf": "9f23f3add06376ba56326a9e3a99223a304c60a5",
    "WaterBottle.bin": "b10cfd0133b85593e01ee7f7b107d85d3ec491a6",
    "WaterBottle_baseColor.png": "f28c3f27d6dcfc1316a6cc00a4111b89cd4438b8",
}


def prepare(output):
    output.mkdir(parents=True, exist_ok=True)
    data = {}
    for name, expected in BLOBS.items():
        path = output / name
        if path.exists():
            blob = path.read_bytes()
        else:
            with urllib.request.urlopen(ROOT + name, timeout=60) as response:
                blob = response.read()
        git_blob = f"blob {len(blob)}\0".encode() + blob
        if hashlib.sha1(git_blob).hexdigest() != expected:
            raise ValueError(f"Unexpected upstream contents for {name}")
        path.write_bytes(blob)
        data[name] = blob
    scene = json.loads(data["WaterBottle.gltf"])
    primitive, = scene["meshes"][0]["primitives"]

    def unpack(index, fmt):
        entry = scene["accessors"][index]
        view = scene["bufferViews"][entry["bufferView"]]
        start = view.get("byteOffset", 0) + entry.get("byteOffset", 0)
        stride = view.get("byteStride", struct.calcsize(fmt))
        return [struct.unpack_from(fmt, data["WaterBottle.bin"], start+i*stride) for i in range(entry["count"])]

    positions = unpack(primitive["attributes"]["POSITION"], "<3f")
    texcoords = unpack(primitive["attributes"]["TEXCOORD_0"], "<2f")
    indices = [i for i, in unpack(primitive["indices"], "<H")]
    welded = dict(zip(dict.fromkeys(positions), range(1, len(positions)+1)))
    kept = 0
    with (output / "WaterBottle.obj").open("w") as dest:
        for p in welded:
            print("v", *p, file=dest)
        for u, v in texcoords:
            print("vt", u, 1-v, file=dest)
        for offset in range(0, len(indices), 3):
            tri = indices[offset:offset+3]
            if len({welded[positions[v]] for v in tri}) != 3:
                continue
            print("f", *(f"{welded[positions[v]]}/{v+1}" for v in tri), file=dest)
            kept += 1
    print(json.dumps(dict(revision=REVISION, original_vertices=len(positions), welded_vertices=len(welded),
                          source_triangles=len(indices)//3, written_triangles=kept, blobs=BLOBS)))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    prepare(parser.parse_args().output)
