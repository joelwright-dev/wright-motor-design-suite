#pragma once
#include "rust/cxx.h"
#include <NCollection_List.hxx>
#include <memory>
#include <utility>

// Generic template constructor
template <typename T, typename... Args> std::unique_ptr<T> construct_unique(Args... args) {
  return std::unique_ptr<T>(new T(args...));
}

// Type casting
template <typename T, typename U> inline U upcast(T src) { return src; }
template <typename T, typename U> inline const U &upcast_ref(const T &src) { return src; }

// Generic List
template <typename T> std::unique_ptr<std::vector<T>> list_to_vector(const NCollection_List<T> &list) {
  return std::unique_ptr<std::vector<T>>(new std::vector<T>(list.begin(), list.end()));
}

// Generic over the handle type (not over opencascade::handle<T>) so that cxx can take its
// address for MSVC's derived Handle_X classes as well as for the typedef on other platforms.
template <typename H> const typename H::element_type &handle_try_deref(const H &handle) {
  if (handle.IsNull()) {
    throw std::runtime_error("null handle dereference");
  }
  return *handle;
}

// Heap-allocate an OCCT handle of type H (Handle_X) from anything that converts to the underlying
// opencascade::handle<X>: a maker object, a const handle reference, or a raw pointer.
// On MSVC, Handle_X is a class derived from opencascade::handle<X> whose constructors do not
// accept maker objects, so the base handle is built first and then moved into the wrapper.
// Elsewhere Handle_X is a typedef and this is a plain copy.
template <typename H, typename A> inline H *new_handle(A &&arg) {
  return new H(opencascade::handle<typename H::element_type>(std::forward<A>(arg)));
}
