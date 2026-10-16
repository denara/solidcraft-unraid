# Export the current view as DXF

Choose **File → Export Current View as DXF…**, choose whether to omit hidden edges or
export them as dashed lines, then choose **Export DXF…** and a destination. The hidden-edge
choice is remembered between runs. The Japanese menu is **ファイル → 現在の表示をDXFでエクスポート…**.

The drawing uses the camera when Export is pressed, including roll, the world's up axis
and perspective. Only shown bodies are exported, at their current component/occurrence
placements. Hidden bodies neither draw nor hide other bodies. The complete projected
geometry is exported, including geometry beyond the viewport's edges. Grid, sketches,
dimensions, selection highlights and other overlays are excluded. Turn off section analysis
before exporting.

Coordinates are 2D, Z=0, in millimetres. Orthographic views retain 1:1 dimensions in the
view plane. Perspective views retain foreshortening, projected onto the plane through the
camera target; they are intended for illustrations rather than dimensioned fabrication.
Curves and curved silhouettes are approximated with straight segments at a default
model-space chord tolerance of 0.01 mm. This is not an exact analytic curve export.

The AutoCAD 2000 ASCII DXF contains `VISIBLE` (continuous) and, when requested, `HIDDEN`
(2 mm dashes / 1 mm gaps) layers. Hidden curves use connected `LWPOLYLINE` entities with
continuous linetype generation so the pattern does not restart on each chord. The pattern
is embedded in the file. The polyline flags follow the
[Autodesk DXF reference](https://help.autodesk.com/cloudhelp/2017/ENU/AutoCAD-DXF/files/GUID-748FC305-F3F2-4F74-825A-61F04D757A50.htm).

Edges are split where another surface occludes them, including partial occlusion by another
body. Hidden edges are never included in omit mode. Triangle diagonals and smooth seams
are excluded; curved-body silhouettes are included. Very complex drawings fail with an
error rather than returning an incomplete export. If geometry crosses the perspective
camera's eye plane, zoom out before exporting.

Programmatic callers use `file.export_view_dxf` with `camera: Camera`, `hidden: "omit"` or
`"dashed"`, optional `tolerance` (0.0001–1 mm) and optional `path`. Without `path`, the
command returns the DXF text. The command does not modify the design or its undo history.
