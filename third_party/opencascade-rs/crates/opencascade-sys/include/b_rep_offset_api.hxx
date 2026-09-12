#include <BRepOffsetAPI_MakeOffset.hxx>
#include <BRepOffsetAPI_MakePipe.hxx>
#include <BRepOffsetAPI_MakePipeShell.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <Law_Function.hxx>
#include <TopTools_ListOfShape.hxx>
#include <TopoDS_Shape.hxx>
#include <bindings_common.hxx>

// MSVC: Handle_Law_Function is a class derived from opencascade::handle<Law_Function>, so cxx
// cannot bind the member function directly. Free-function wrapper instead.
inline void BRepOffsetAPI_MakePipeShell_SetLaw(BRepOffsetAPI_MakePipeShell &pipe_shell, const TopoDS_Shape &profile,
                                               const Handle_Law_Function &law, bool with_contact,
                                               bool with_correction) {
  pipe_shell.SetLaw(profile, law, with_contact, with_correction);
}
