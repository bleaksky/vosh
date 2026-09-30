"""Pack PNG frames into a Windows .ico, each frame stored as its own PNG, byte for byte.

    python3 -B ico.py out.ico 32.png 16.png 20.png ...

Frames keep the order given. Put 32 first: tauri-codegen takes the first frame as the Windows window icon.
Each PNG must be square, 8 bit RGBA, and 256 px or smaller. Standard library only."""
import struct
import sys


def png_size(data, path):
    if data[:8] != b"\x89PNG\r\n\x1a\n" or data[12:16] != b"IHDR":
        sys.exit(f"{path} is not a PNG")
    w, h, depth, color = struct.unpack(">IIBB", data[16:26])
    if w != h or w > 256 or depth != 8 or color != 6:
        sys.exit(f"{path} must be square, 8 bit RGBA and at most 256 px, found {w}x{h} depth {depth} type {color}")
    return w


def main(out, frames):
    blobs = []
    for path in frames:
        with open(path, "rb") as f:
            data = f.read()
        blobs.append((png_size(data, path), data))
    head = struct.pack("<HHH", 0, 1, len(blobs))
    offset = len(head) + 16 * len(blobs)
    entries, body = b"", b""
    for size, data in blobs:
        side = 0 if size == 256 else size
        entries += struct.pack("<BBBBHHII", side, side, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
        body += data
    with open(out, "wb") as f:
        f.write(head + entries + body)


if __name__ == "__main__":
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2:])
