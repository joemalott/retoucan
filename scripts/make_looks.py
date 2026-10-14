"""Generate Retoucan's starter looks as Lightroom develop presets (.xmp).

These are original settings written for Retoucan and released under the
project's MIT license. Run: python3 scripts/make_looks.py
"""
import hashlib
from pathlib import Path

LOOKS = {
    "Golden Film": dict(
        Contrast2012=-10, Highlights2012=-25, Shadows2012=20, Whites2012=10, Blacks2012=8,
        ParametricDarks=-5, ParametricLights=5,
        SaturationAdjustmentOrange=-5, LuminanceAdjustmentOrange=8,
        HueAdjustmentYellow=-10, SaturationAdjustmentYellow=-15,
        HueAdjustmentGreen=15, SaturationAdjustmentGreen=-35, LuminanceAdjustmentGreen=-5,
        SaturationAdjustmentBlue=-15,
        SplitToningShadowHue=210, SplitToningShadowSaturation=8,
        SplitToningHighlightHue=40, SplitToningHighlightSaturation=15, SplitToningBalance=20,
        GrainAmount=15, GrainSize=25, GrainFrequency=50,
        curve=[(0, 18), (64, 62), (128, 130), (192, 198), (255, 248)],
    ),
    "Soft Pastel": dict(
        Contrast2012=-25, Highlights2012=-30, Shadows2012=35, Whites2012=5, Blacks2012=20,
        Vibrance=-10, Saturation=-10,
        LuminanceAdjustmentOrange=10, SaturationAdjustmentGreen=-40, LuminanceAdjustmentGreen=15,
        SaturationAdjustmentBlue=-20, LuminanceAdjustmentBlue=15,
        SplitToningHighlightHue=30, SplitToningHighlightSaturation=6,
        SplitToningShadowHue=200, SplitToningShadowSaturation=6,
        curve=[(0, 30), (128, 136), (255, 245)],
    ),
    "Warm Negative": dict(
        Contrast2012=-15, Highlights2012=-20, Shadows2012=25,
        HueAdjustmentRed=10, HueAdjustmentOrange=5, LuminanceAdjustmentOrange=10, SaturationAdjustmentOrange=-5,
        HueAdjustmentYellow=-15, SaturationAdjustmentYellow=-20,
        HueAdjustmentGreen=25, SaturationAdjustmentGreen=-40,
        HueAdjustmentAqua=-10, HueAdjustmentBlue=-10, SaturationAdjustmentBlue=-15,
        SplitToningHighlightHue=45, SplitToningHighlightSaturation=12,
        SplitToningShadowHue=190, SplitToningShadowSaturation=6,
        GrainAmount=18, GrainSize=25, GrainFrequency=50,
        curve=[(0, 12), (60, 58), (128, 132), (200, 205), (255, 250)],
    ),
    "Clean Bright": dict(
        Exposure2012=0.2, Contrast2012=5, Highlights2012=-20, Shadows2012=15, Whites2012=15, Blacks2012=-5,
        Vibrance=10, LuminanceAdjustmentOrange=5, SaturationAdjustmentOrange=-5,
    ),
    "Moody Matte": dict(
        Exposure2012=-0.2, Contrast2012=10, Highlights2012=-40, Shadows2012=10, Blacks2012=15, Clarity2012=5,
        Vibrance=-15, SaturationAdjustmentGreen=-50, LuminanceAdjustmentGreen=-20, SaturationAdjustmentBlue=-30,
        SplitToningShadowHue=200, SplitToningShadowSaturation=12,
        SplitToningHighlightHue=35, SplitToningHighlightSaturation=10,
        PostCropVignetteAmount=-15, PostCropVignetteMidpoint=50, PostCropVignetteFeather=60,
        curve=[(0, 25), (70, 65), (180, 185), (255, 240)],
    ),
    "Teal & Amber": dict(
        Contrast2012=15, Highlights2012=-30, Blacks2012=-5, Saturation=-10,
        SaturationAdjustmentOrange=5, HueAdjustmentAqua=10, HueAdjustmentGreen=30, SaturationAdjustmentGreen=-30,
        SplitToningShadowHue=195, SplitToningShadowSaturation=20,
        SplitToningHighlightHue=40, SplitToningHighlightSaturation=15,
    ),
    "Faded Vintage": dict(
        Contrast2012=-10, Saturation=-35,
        SplitToningShadowHue=38, SplitToningShadowSaturation=18,
        SplitToningHighlightHue=38, SplitToningHighlightSaturation=18,
        GrainAmount=25, GrainSize=30, GrainFrequency=50,
        PostCropVignetteAmount=-20, PostCropVignetteMidpoint=50, PostCropVignetteFeather=70,
        curve=[(0, 35), (128, 130), (255, 235)],
    ),
    # Muted, slightly vintage film: olive/sage greens, matte blacks, creamy
    # (not pure) whites, natural warm skin. Garden and daylight scenes.
    "Sage Film": dict(
        Contrast2012=-12, Highlights2012=-35, Shadows2012=20, Whites2012=-15, Blacks2012=5,
        Vibrance=-5, Saturation=-8,
        HueAdjustmentOrange=2, SaturationAdjustmentOrange=-5, LuminanceAdjustmentOrange=8,
        HueAdjustmentYellow=-15, SaturationAdjustmentYellow=-25, LuminanceAdjustmentYellow=-5,
        HueAdjustmentGreen=-15, SaturationAdjustmentGreen=-35, LuminanceAdjustmentGreen=-12,
        SaturationAdjustmentAqua=-30, SaturationAdjustmentBlue=-25, LuminanceAdjustmentBlue=5,
        SplitToningShadowHue=160, SplitToningShadowSaturation=6,
        SplitToningHighlightHue=45, SplitToningHighlightSaturation=8, SplitToningBalance=10,
        GrainAmount=12, GrainSize=25, GrainFrequency=50,
        curve=[(0, 22), (64, 66), (128, 128), (192, 190), (255, 242)],
    ),
    # Warm sunset film: golden highlights, warm brown shadows, khaki/olive
    # greens, soft contrast that never goes fully black.
    "Golden Hour Film": dict(
        Contrast2012=-8, Highlights2012=-30, Shadows2012=15, Whites2012=-10, Blacks2012=8,
        Vibrance=-10, Saturation=-5,
        HueAdjustmentRed=5, SaturationAdjustmentRed=-10,
        HueAdjustmentOrange=3, SaturationAdjustmentOrange=-5, LuminanceAdjustmentOrange=5,
        HueAdjustmentYellow=-15, SaturationAdjustmentYellow=-25,
        HueAdjustmentGreen=-25, SaturationAdjustmentGreen=-45, LuminanceAdjustmentGreen=-15,
        SaturationAdjustmentAqua=-35, SaturationAdjustmentBlue=-35,
        SplitToningShadowHue=30, SplitToningShadowSaturation=10,
        SplitToningHighlightHue=42, SplitToningHighlightSaturation=20, SplitToningBalance=20,
        GrainAmount=18, GrainSize=25, GrainFrequency=50,
        PostCropVignetteAmount=-10, PostCropVignetteMidpoint=50, PostCropVignetteFeather=70,
        curve=[(0, 18), (64, 60), (128, 130), (192, 196), (255, 245)],
    ),
    "Classic B&W": dict(
        ConvertToGrayscale=True, Contrast2012=20, Highlights2012=-15, Shadows2012=10, Clarity2012=10,
        GrayMixerRed=10, GrayMixerOrange=15, GrayMixerYellow=5, GrayMixerGreen=-20, GrayMixerBlue=-15,
        GrainAmount=20, GrainSize=25, GrainFrequency=50,
        curve=[(0, 5), (64, 56), (192, 200), (255, 255)],
    ),
}


def uuid(name):
    return hashlib.md5(f"retoucan-look-{name}".encode()).hexdigest().upper()


def fmt(v):
    if isinstance(v, bool):
        return "True" if v else "False"
    if isinstance(v, float):
        return f"{v:+.2f}"
    return f"{v:+d}" if v else "0"


def xmp(name, params, sort):
    params = dict(params)
    curve = params.pop("curve", None)
    attrs = "".join(f'   crs:{k}="{fmt(v)}"\n' for k, v in params.items())
    if curve:
        attrs += '   crs:ToneCurveName2012="Custom"\n'
    curve_xml = ""
    if curve:
        lis = "".join(f"     <rdf:li>{x}, {y}</rdf:li>\n" for x, y in curve)
        curve_xml = f"   <crs:ToneCurvePV2012>\n    <rdf:Seq>\n{lis}    </rdf:Seq>\n   </crs:ToneCurvePV2012>\n"
    alt = lambda tag, text: (
        f"   <crs:{tag}>\n    <rdf:Alt>\n     <rdf:li xml:lang=\"x-default\">{text}</rdf:li>\n    </rdf:Alt>\n   </crs:{tag}>\n"
    )
    return f"""<x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="Retoucan">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
   crs:PresetType="Normal"
   crs:UUID="{uuid(name)}"
   crs:SupportsAmount2="True"
   crs:SupportsAmount="True"
   crs:SupportsColor="True"
   crs:SupportsMonochrome="True"
   crs:SupportsHighDynamicRange="True"
   crs:SupportsNormalDynamicRange="True"
   crs:SupportsSceneReferred="True"
   crs:SupportsOutputReferred="True"
   crs:RequiresRGBTables="False"
   crs:CameraModelRestriction=""
   crs:Copyright="Retoucan (MIT)"
   crs:ContactInfo="https://github.com/joemalott/retoucan"
   crs:Version="15.0"
   crs:ProcessVersion="11.0"
{attrs}   crs:HasSettings="True">
{alt("Name", name)}{alt("ShortName", name)}{alt("SortName", f"Retoucan Look {sort:02}")}{alt("Group", "Retoucan Looks")}{curve_xml}  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
"""


out = Path(__file__).resolve().parent.parent / "looks"
out.mkdir(exist_ok=True)
for i, (name, params) in enumerate(LOOKS.items()):
    fname = name.replace("&", "and").replace(" ", "-").lower() + ".xmp"
    (out / fname).write_text(xmp(name, params, i))
    print(fname)
