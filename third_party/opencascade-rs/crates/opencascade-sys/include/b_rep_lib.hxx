#include <BRepLib.hxx>
#include <BRepLib_ToolTriangulatedShape.hxx>
#include <TopoDS_Shape.hxx>
#include <bindings_common.hxx>

// MSVC: Handle_Poly_Triangulation is a class derived from opencascade::handle<Poly_Triangulation>
// rather than a typedef, so cxx cannot take the address of the OCCT static directly.
inline void BRepLib_ToolTriangulatedShape_ComputeNormals(const TopoDS_Face &face,
                                                          const Handle_Poly_Triangulation &triangulation) {
  BRepLib_ToolTriangulatedShape::ComputeNormals(face, triangulation);
}
