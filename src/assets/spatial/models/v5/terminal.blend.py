"""3D source model for AI Manager's home-page terminal.

    Blender -b -P terminal.py -- <out_dir> <yaw_start> <yaw_end> <frames> <res>

With no arguments, renders a single front-facing preview frame.
"""
import bpy, math, sys, os
from mathutils import Vector

# ---------- infrastructure ----------

def srgb(c):
    """Blender's Base Color takes linear values. Feeding it sRGB numbers
    directly makes everything look brighter and washed out — ivory turns
    pure white, mint green fades to pale green."""
    f = lambda v: v / 12.92 if v <= 0.04045 else ((v + 0.055) / 1.055) ** 2.4
    return tuple(f(v) for v in c)

def smooth(angle=35.0):
    try:
        bpy.ops.object.shade_smooth_by_angle(angle=math.radians(angle))
    except Exception:
        bpy.ops.object.shade_smooth()

def box(name, size, bevel, location=(0, 0, 0), segments=6):
    bpy.ops.mesh.primitive_cube_add(size=1, location=location)
    ob = bpy.context.active_object
    ob.name = name
    ob.scale = Vector(size)
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    m = ob.modifiers.new("bev", 'BEVEL')
    m.width = bevel
    m.segments = segments
    m.limit_method = 'ANGLE'
    m.angle_limit = math.radians(40)
    m.harden_normals = True
    smooth()
    return ob

def clay(name, rgb, rough=0.82, sss=0.0, emit=0.0):
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    b = next(n for n in mat.node_tree.nodes if n.type == 'BSDF_PRINCIPLED')
    b.inputs["Base Color"].default_value = (*srgb(rgb), 1)
    b.inputs["Roughness"].default_value = rough
    b.inputs["Metallic"].default_value = 0.0
    if "IOR" in b.inputs:
        b.inputs["IOR"].default_value = 1.42
    if "Subsurface Weight" in b.inputs:
        b.inputs["Subsurface Weight"].default_value = sss
    if sss and "Subsurface Radius" in b.inputs:
        b.inputs["Subsurface Radius"].default_value = (0.32, 0.24, 0.17)
    if emit:
        b.inputs["Emission Color"].default_value = (*srgb(rgb), 1)
        b.inputs["Emission Strength"].default_value = emit
    return mat

bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene

IVORY = (0.898, 0.851, 0.733)
MINT  = (0.482, 0.816, 0.463)
GLYPH = (0.729, 0.945, 0.706)

m_body  = clay("body",   IVORY, 0.78, sss=0.18)
m_scr   = clay("screen", MINT,  0.60, sss=0.12)
m_glyph = clay("glyph",  GLYPH, 0.52, sss=0.08, emit=0.10)

# ---------- geometry ----------
# The camera sits on the -Y side, so the front face is the one with the smallest y.

BODY_Y = 0.55          # body half-thickness
REC_Y  = -0.40         # recess floor (the plane the screen sits on)

body = box("body", (1.56, BODY_Y * 2, 1.26), 0.155, location=(0, 0, 1.00))
body.data.materials.append(m_body)

# Cuts the recess out of the front panel using a box with its own rounded
# corners. The screen can't just be "placed inside the body" — a solid body
# would hide it entirely, which is exactly why the screen disappeared in the
# previous version.
cutter = box("cutter", (1.19, 0.34, 0.88), 0.075, location=(0, REC_Y - 0.17, 1.02))
bpy.context.view_layer.objects.active = cutter
bpy.ops.object.modifier_apply(modifier="bev")
bl = body.modifiers.new("recess", 'BOOLEAN')
bl.operation = 'DIFFERENCE'
bl.object = cutter
bl.solver = 'EXACT'
cutter.hide_render = True
cutter.hide_viewport = True

# The screen sits at the bottom of the recess, slightly smaller than the
# recess on every side, exposing the inner wall to form the frame's inner shadow.
screen = box("screen", (1.11, 0.12, 0.80), 0.055, location=(0, REC_Y + 0.02, 1.02))
screen.data.materials.append(m_scr)

SCR_FRONT = REC_Y + 0.02 - 0.06      # screen front face
TIP_X, TIP_Z, BAR_L, BAR_A = -0.085, 1.025, 0.365, math.radians(41)
for sign in (+1, -1):
    cx = TIP_X - math.sin(BAR_A) * BAR_L / 2
    cz = TIP_Z + sign * math.cos(BAR_A) * BAR_L / 2
    bpy.ops.mesh.primitive_cube_add(size=1, location=(cx, SCR_FRONT + 0.02, cz))
    bar = bpy.context.active_object
    bar.scale = (0.078, 0.062, BAR_L)
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    bar.rotation_euler = (0, -sign * BAR_A, 0)
    bm = bar.modifiers.new("bev", 'BEVEL')
    bm.width = 0.032
    bm.segments = 5
    bm.harden_normals = True
    smooth()
    bar.data.materials.append(m_glyph)

cursor = box("cursor", (0.165, 0.062, 0.165), 0.038,
             location=(0.205, SCR_FRONT + 0.02, 0.918))
cursor.data.materials.append(m_glyph)

bpy.ops.mesh.primitive_cylinder_add(radius=0.365, depth=0.33, vertices=64,
                                    location=(0, 0.02, 0.21))
neck = bpy.context.active_object
nb = neck.modifiers.new("bev", 'BEVEL')
nb.width = 0.06
nb.segments = 5
nb.harden_normals = True
smooth()
neck.data.materials.append(m_body)

base = box("base", (1.92, 1.24, 0.26), 0.105, location=(0, 0.03, 0.05))
base.data.materials.append(m_body)

# The turntable sequence needs a cast shadow (rendered as a true projection
# from that angle); the icon and sidebar mark need only the object itself.
if "noshadow" not in sys.argv:
    bpy.ops.mesh.primitive_plane_add(size=26, location=(0, 0, -0.078))
    bpy.context.active_object.is_shadow_catcher = True

# ---------- lighting ----------

def area(name, loc, rot, size, energy, color=(1, 1, 1)):
    d = bpy.data.lights.new(name, 'AREA')
    d.size, d.energy, d.color = size, energy, color
    o = bpy.data.objects.new(name, d)
    o.location, o.rotation_euler = loc, rot
    scene.collection.objects.link(o)

area("key",  (-2.6, -3.0, 4.4), (math.radians(38), 0, math.radians(-38)), 5.5, 210)
area("fill", ( 3.4, -2.2, 1.9), (math.radians(74), 0, math.radians(56)),  4.5, 74, (0.94, 0.98, 1.0))
area("rim",  ( 0.4,  3.4, 2.6), (math.radians(-58), 0, 0),                4.0, 96, (0.90, 1.0, 0.94))

world = bpy.data.worlds.new("w")
scene.world = world
world.use_nodes = True
bgn = next((n for n in world.node_tree.nodes if n.type == 'BACKGROUND'), None)
if bgn is None:
    bgn = world.node_tree.nodes.new('ShaderNodeBackground')
    out = next(n for n in world.node_tree.nodes if n.type == 'OUTPUT_WORLD')
    world.node_tree.links.new(bgn.outputs[0], out.inputs[0])
bgn.inputs[0].default_value = (0.90, 0.94, 0.92, 1)
bgn.inputs[1].default_value = 0.30

# ---------- camera ----------

cam_d = bpy.data.cameras.new("cam")
cam_d.lens = 115
cam = bpy.data.objects.new("cam", cam_d)
scene.collection.objects.link(cam)
scene.camera = cam
DIST, PITCH, LOOK_Z = 8.75, 13.0, 0.775

def place(yaw_deg):
    y, p = math.radians(yaw_deg), math.radians(PITCH)
    cam.location = (DIST * math.sin(y) * math.cos(p),
                    -DIST * math.cos(y) * math.cos(p),
                    LOOK_Z + DIST * math.sin(p))
    cam.rotation_euler = (math.radians(90) - p, 0, y)

# ---------- rendering ----------

scene.view_settings.view_transform = 'Standard'
scene.render.engine = 'CYCLES'
try:
    scene.cycles.device = 'GPU'
    prefs = bpy.context.preferences.addons['cycles'].preferences
    prefs.compute_device_type = 'METAL'
    prefs.get_devices()
    for d in prefs.devices:
        d.use = True
except Exception as e:
    print("GPU setup skipped:", e)
scene.cycles.samples = 96
scene.cycles.use_denoising = True
scene.render.film_transparent = True
scene.render.image_settings.file_format = 'PNG'
scene.render.image_settings.color_mode = 'RGBA'

argv = [a for a in (sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else [])
        if a != "noshadow"]
if len(argv) >= 5:
    out_dir, y0, y1, n, res = argv[0], float(argv[1]), float(argv[2]), int(argv[3]), int(argv[4])
    os.makedirs(out_dir, exist_ok=True)
    scene.render.resolution_x = scene.render.resolution_y = res
    for i in range(n):
        place(y0 if n == 1 else y0 + (y1 - y0) * i / (n - 1))
        scene.render.filepath = os.path.join(out_dir, f"f{i:02d}.png")
        bpy.ops.render.render(write_still=True)
else:
    scene.render.resolution_x = scene.render.resolution_y = 900
    place(0.0)
    scene.render.filepath = "/tmp/blend/probe.png"
    bpy.ops.render.render(write_still=True)
print("RENDER OK")
