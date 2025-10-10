"""Read embedded EXE icons as data and compare them with the original ICO. No GUI launch."""
import argparse
import ctypes
from ctypes import wintypes
from pathlib import Path
import struct
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("exe", type=Path)
    args = parser.parse_args()
    if sys.platform != "win32":
        parser.error("Windows resource loading is required")
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.LoadLibraryExW.argtypes = [wintypes.LPCWSTR, wintypes.HANDLE, wintypes.DWORD]
    kernel.LoadLibraryExW.restype = wintypes.HMODULE
    kernel.FindResourceW.argtypes = [wintypes.HMODULE, ctypes.c_void_p, ctypes.c_void_p]
    kernel.FindResourceW.restype = wintypes.HANDLE
    kernel.SizeofResource.argtypes = [wintypes.HMODULE, wintypes.HANDLE]
    kernel.SizeofResource.restype = wintypes.DWORD
    kernel.LoadResource.argtypes = [wintypes.HMODULE, wintypes.HANDLE]
    kernel.LoadResource.restype = wintypes.HANDLE
    kernel.LockResource.argtypes = [wintypes.HANDLE]
    kernel.LockResource.restype = ctypes.c_void_p
    kernel.FreeLibrary.argtypes = [wintypes.HMODULE]
    kernel.FreeLibrary.restype = wintypes.BOOL
    # Map resources only; this never executes code in the target application.
    module = kernel.LoadLibraryExW(str(args.exe.resolve(strict=True)), None, 0x22)
    if not module:
        raise ctypes.WinError(ctypes.get_last_error())

    def resource(kind, ident):
        entry = kernel.FindResourceW(module, ident, kind)
        if not entry:
            raise ctypes.WinError(ctypes.get_last_error())
        size = kernel.SizeofResource(module, entry)
        handle = kernel.LoadResource(module, entry)
        data = kernel.LockResource(handle)
        if not data or not size:
            raise RuntimeError("Empty or unreadable icon resource")
        return ctypes.string_at(data, size)

    try:
        ico = (Path(__file__).resolve().parent.parent / "assets/app-icon.ico").read_bytes()
        group = resource(14, 1)  # RT_GROUP_ICON, the default winresource icon ID.
        assert struct.unpack_from("<HH", group) == (0, 1)
        count = struct.unpack_from("<H", group, 4)[0]
        assert count == struct.unpack_from("<H", ico, 4)[0] > 1
        sizes = []
        for index in range(count):
            embedded = struct.unpack_from("<BBBBHHIH", group, 6 + index * 14)
            original = struct.unpack_from("<BBBBHHII", ico, 6 + index * 16)
            assert embedded[:7] == original[:7], f"Icon size or format differs: {embedded}, {original}"
            payload = resource(3, embedded[7])  # RT_ICON.
            assert payload == ico[original[7]:original[7] + original[6]], "Icon pixels differ"
            sizes.append(embedded[0] or 256)
        from PIL import Image
        with Image.open(Path(__file__).resolve().parent.parent / "assets/app-icon.ico") as image:
            for size in sizes:
                bitmap = image.ico.getimage((size, size)).convert("RGBA")
                assert bitmap.getchannel("A").getextrema() == (0, 255)
                assert len(bitmap.getcolors(size * size)) > 10
        print(f"Embedded icon verified: {count} RGBA sizes {sizes}, identical to source ICO")
    finally:
        kernel.FreeLibrary(module)


if __name__ == "__main__":
    main()
