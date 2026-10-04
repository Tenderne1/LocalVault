from PIL import Image, ImageDraw
import os

out = r"H:\passwordmanagers\LocalVault-v1.9.3\extension"
os.makedirs(out, exist_ok=True)

for size in (16, 48, 128):
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    s = size
    # 圆角矩形背景（品牌蓝）
    d.rounded_rectangle([s*0.06, s*0.06, s*0.94, s*0.94], radius=s*0.22, fill=(37, 99, 235, 255))
    white = (255, 255, 255, 255)
    w = max(1, int(s*0.085))
    # 钥匙环
    cx, cy, r = s*0.5, s*0.37, s*0.15
    d.ellipse([cx-r, cy-r, cx+r, cy+r], outline=white, width=w)
    # 钥匙杆
    d.rectangle([cx-w/2, cy+r-w/2, cx+w/2, s*0.72], fill=white)
    # 底部齿
    d.rectangle([s*0.5, s*0.60, s*0.72, s*0.72], fill=white)
    path = os.path.join(out, f"icon{size}.png")
    img.save(path)
    print("saved", path, img.size)
