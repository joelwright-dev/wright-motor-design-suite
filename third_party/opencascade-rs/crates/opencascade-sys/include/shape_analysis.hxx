#include <ShapeAnalysis.hxx>
#include <ShapeAnalysis_FreeBounds.hxx>
#include <TopTools_HSequenceOfShape.hxx>
#include <bindings_common.hxx>

// MSVC: Handle_TopTools_HSequenceOfShape is a class derived from the opencascade::handle, so cxx
// cannot bind the OCCT static directly. Free-function wrapper instead.
inline void ShapeAnalysis_FreeBounds_ConnectEdgesToWires(Handle_TopTools_HSequenceOfShape &edges,
                                                         const Standard_Real tolerance, const Standard_Boolean shared,
                                                         Handle_TopTools_HSequenceOfShape &wires) {
  ShapeAnalysis_FreeBounds::ConnectEdgesToWires(edges, tolerance, shared, wires);
}
