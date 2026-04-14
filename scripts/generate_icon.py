#!/usr/bin/env python3
"""Generate the application icon for FerriScribe.

Produces a 1024x1024 PNG that can be fed to `npx tauri icon` to create
all platform-specific icon variants.
"""

from PIL import Image, ImageDraw, ImageFont
import math

SIZE = 1024
CENTER = SIZE // 2

img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
draw = ImageDraw.Draw(img)

# -- Background: rounded rectangle with gradient feel --
# Use a rich teal/medical blue
bg_color = (26, 115, 168)       # primary teal-blue
bg_dark = (18, 85, 130)         # darker shade for depth
accent = (255, 255, 255)        # white for the icon elements
highlight = (72, 199, 220)      # lighter teal for accents

# Draw rounded rectangle background
corner_radius = 200
draw.rounded_rectangle(
    [(40, 40), (SIZE - 40, SIZE - 40)],
    radius=corner_radius,
    fill=bg_color,
)

# Subtle inner shadow / gradient ring
draw.rounded_rectangle(
    [(55, 55), (SIZE - 55, SIZE - 55)],
    radius=corner_radius - 12,
    fill=None,
    outline=(255, 255, 255, 25),
    width=3,
)

# -- Draw medical cross --
cross_cx, cross_cy = CENTER, CENTER - 30
cross_arm_w = 100   # half-width of each arm
cross_arm_h = 180   # half-height of each arm
cross_r = 28        # corner radius

# Vertical bar
draw.rounded_rectangle(
    [(cross_cx - cross_arm_w, cross_cy - cross_arm_h),
     (cross_cx + cross_arm_w, cross_cy + cross_arm_h)],
    radius=cross_r,
    fill=accent,
)
# Horizontal bar
draw.rounded_rectangle(
    [(cross_cx - cross_arm_h, cross_cy - cross_arm_w),
     (cross_cx + cross_arm_h, cross_cy + cross_arm_w)],
    radius=cross_r,
    fill=accent,
)

# Inner cross (creates a "hollow" cross look)
inner_inset = 30
inner_r = 18
inner_color = bg_color

# Vertical inner
draw.rounded_rectangle(
    [(cross_cx - cross_arm_w + inner_inset, cross_cy - cross_arm_h + inner_inset),
     (cross_cx + cross_arm_w - inner_inset, cross_cy + cross_arm_h - inner_inset)],
    radius=inner_r,
    fill=inner_color,
)
# Horizontal inner
draw.rounded_rectangle(
    [(cross_cx - cross_arm_h + inner_inset, cross_cy - cross_arm_w + inner_inset),
     (cross_cx + cross_arm_h - inner_inset, cross_cy + cross_arm_w - inner_inset)],
    radius=inner_r,
    fill=inner_color,
)

# -- Draw a stethoscope-inspired circular element at the bottom --
stetho_cy = CENTER + 260
stetho_cx = CENTER
stetho_r_outer = 80
stetho_r_inner = 55

# Outer ring
draw.ellipse(
    [(stetho_cx - stetho_r_outer, stetho_cy - stetho_r_outer),
     (stetho_cx + stetho_r_outer, stetho_cy + stetho_r_outer)],
    fill=accent,
)
# Inner circle
draw.ellipse(
    [(stetho_cx - stetho_r_inner, stetho_cy - stetho_r_inner),
     (stetho_cx + stetho_r_inner, stetho_cy + stetho_r_inner)],
    fill=bg_color,
)
# Small highlight dot in the center
dot_r = 18
draw.ellipse(
    [(stetho_cx - dot_r, stetho_cy - dot_r),
     (stetho_cx + dot_r, stetho_cy + dot_r)],
    fill=highlight,
)

# -- Tube lines connecting cross to stethoscope --
tube_width = 12
tube_color = accent

# Left tube
draw.line(
    [(cross_cx - 60, cross_cy + cross_arm_h - 10),
     (stetho_cx - 40, stetho_cy - stetho_r_outer + 5)],
    fill=tube_color,
    width=tube_width,
)
# Right tube
draw.line(
    [(cross_cx + 60, cross_cy + cross_arm_h - 10),
     (stetho_cx + 40, stetho_cy - stetho_r_outer + 5)],
    fill=tube_color,
    width=tube_width,
)

# -- Small pulse/heartbeat line across the cross center --
pulse_y = cross_cy
pulse_points = []
px_start = cross_cx - 120
px_end = cross_cx + 120
segments = 24
for i in range(segments + 1):
    px = px_start + (px_end - px_start) * i / segments
    t = i / segments
    # Flat baseline with a sharp peak in the middle
    if 0.35 < t < 0.42:
        py = pulse_y - 60 * ((t - 0.35) / 0.07)
    elif 0.42 <= t < 0.5:
        py = pulse_y - 60 + 120 * ((t - 0.42) / 0.08)
    elif 0.5 <= t < 0.55:
        py = pulse_y + 60 - 60 * ((t - 0.5) / 0.05)
    elif 0.55 <= t < 0.65:
        py = pulse_y - 30 * math.sin((t - 0.55) / 0.10 * math.pi)
    else:
        py = pulse_y
    pulse_points.append((px, py))

# Draw pulse with thick line
if len(pulse_points) >= 2:
    draw.line(pulse_points, fill=highlight, width=10, joint="curve")

# -- Top earpieces --
ear_y = cross_cy - cross_arm_h - 30
ear_r = 22
# Left earpiece
draw.ellipse(
    [(cross_cx - 90 - ear_r, ear_y - ear_r),
     (cross_cx - 90 + ear_r, ear_y + ear_r)],
    fill=accent,
)
# Right earpiece
draw.ellipse(
    [(cross_cx + 90 - ear_r, ear_y - ear_r),
     (cross_cx + 90 + ear_r, ear_y + ear_r)],
    fill=accent,
)
# Connecting bar
draw.line(
    [(cross_cx - 90, ear_y), (cross_cx + 90, ear_y)],
    fill=accent,
    width=tube_width,
)
# Tubes from earpieces down to cross
draw.line(
    [(cross_cx - 90, ear_y + ear_r),
     (cross_cx - 60, cross_cy - cross_arm_h + 10)],
    fill=tube_color,
    width=tube_width,
)
draw.line(
    [(cross_cx + 90, ear_y + ear_r),
     (cross_cx + 60, cross_cy - cross_arm_h + 10)],
    fill=tube_color,
    width=tube_width,
)

output_path = "src-tauri/icons/icon.png"
img.save(output_path, "PNG")
print(f"Icon saved to {output_path} ({SIZE}x{SIZE})")
