"""生成 Aurora 应用图标（深蓝底 + 琥珀色雷达扫掠），输出 PNG 与 ICO。"""
import math
import struct
import zlib
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "src-tauri" / "icons"


def render(size: int, ss: int = 3):
    """以 size*ss 超采样绘制后缩小到 size。返回 [(r,g,b,a), ...] 行优先。"""
    S = size * ss
    cx, cy = 0.5, 0.52
    px = [None] * (S * S)

    def rr(x, y, w, h, r):
        # 圆角矩形 SDF，返回布尔
        qx = abs(x) - (w / 2 - r)
        qy = abs(y) - (h / 2 - r)
        dx = max(qx, 0.0)
        dy = max(qy, 0.0)
        d = math.hypot(dx, dy) + min(max(qx, qy), 0.0) - r
        return d <= 0

    for j in range(S):
        for i in range(S):
            x = (i + 0.5) / S
            y = (j + 0.5) / S
            nx, ny = x - cx, y - cy
            if not rr(nx, ny - 0.0, 0.94, 0.94, 0.24):
                continue
            # 背景竖向渐变
            t = (y - 0.03) / 0.94
            t = min(max(t, 0.0), 1.0)
            r = int(26 + (11 - 26) * t)
            g = int(34 + (16 - 34) * t)
            b = int(51 + (30 - 51) * t)
            d = math.hypot(nx, ny)
            # 雷达内圈细环
            for rad, w, col in ((0.16, 0.014, (44, 58, 88)), (0.30, 0.014, (44, 58, 88))):
                ring = abs(d - rad) < w / 2
                if ring:
                    a = 1.0
                    r = int(r * (1 - a) + col[0] * a)
                    g = int(g * (1 - a) + col[1] * a)
                    b = int(b * (1 - a) + col[2] * a)
            # 琥珀主弧（-30°..75°）
            ang = math.degrees(math.atan2(-ny, nx))
            if -30 <= ang <= 75 and abs(d - 0.30) < 0.028:
                edge = min(1.0, (0.028 - abs(d - 0.30)) / 0.006)
                r = int(r * (1 - edge) + 242 * edge)
                g = int(g * (1 - edge) + 163 * edge)
                b = int(b * (1 - edge) + 60 * edge)
            # 弧端点目标亮点
            tx, ty = cx + 0.30 * math.cos(math.radians(-30)), cy - 0.30 * math.sin(math.radians(-30))
            if math.hypot(x - tx, y - ty) < 0.030:
                a = min(1.0, 0.030 - math.hypot(x - tx, y - ty)) / 0.030 * 0 + 1.0
                r, g, b = 255, 214, 138
            # 中心点
            if d < 0.035:
                a = min(1.0, (0.035 - d) / 0.008) if d > 0.027 else 1.0
                r = int(r * (1 - a) + 83 * a)
                g = int(g * (1 - a) + 193 * a)
                b = int(b * (1 - a) + 222 * a)
            px[j * S + i] = (r, g, b, 255)

    # 超采样缩小
    out = []
    for j in range(size):
        for i in range(size):
            acc = [0, 0, 0, 0]
            for dj in range(ss):
                for di in range(ss):
                    c = px[(j * ss + dj) * S + (i * ss + di)]
                    if c is None:
                        continue
                    for k in range(4):
                        acc[k] += c[k]
            out.append(tuple(min(255, max(0, v // (ss * ss))) for v in acc))
    return out


def png(size: int, data) -> bytes:
    # 每行开头一个过滤字节 0（None），随后是逐像素 RGBA
    raw = bytearray()
    i = 0
    for _row in range(size):
        raw += b"\x00"
        for _col in range(size):
            r, g, b, a = (min(255, max(0, int(v))) for v in data[i])
            i += 1
            raw += struct.pack("4B", r, g, b, a)
    def chunk(tag, payload):
        return (
            struct.pack(">I", len(payload))
            + tag
            + payload
            + struct.pack(">I", zlib.crc32(tag + payload) & 0xFFFFFFFF)
        )
    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", ihdr)
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )


def ico(sizes_png: list[tuple[int, bytes]]) -> bytes:
    header = struct.pack("<HHH", 0, 1, len(sizes_png))
    entries = b""
    offset = 6 + 16 * len(sizes_png)
    blobs = b""
    for size, data in sizes_png:
        entries += struct.pack(
            "<BBBBHHII",
            size % 256,  # ICO 规范：256px 存 0
            size % 256,
            0, 0, 1, 32,
            len(data),
            offset,
        )
        offset += len(data)
        blobs += data
    return header + entries + blobs


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    big = render(512)
    (OUT / "icon.png").write_bytes(png(512, big))
    # 缩小得到常用尺寸
    def scale(data, frm, to):
        out = []
        for j in range(to):
            for i in range(to):
                si = i * frm // to
                sj = j * frm // to
                out.append(data[sj * frm + si])
        return out
    p128 = png(128, scale(big, 512, 128))
    (OUT / "128x128.png").write_bytes(p128)
    p32 = png(32, scale(big, 512, 32))
    (OUT / "32x32.png").write_bytes(p32)
    p256 = png(256, scale(big, 512, 256))
    (OUT / "icon.ico").write_bytes(ico([(256, p256), (32, p32)]))
    print("icons written to", OUT)


if __name__ == "__main__":
    main()
