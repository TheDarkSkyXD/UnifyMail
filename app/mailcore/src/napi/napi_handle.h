//
//  napi_handle.h
//  mailcore-napi
//
//  N-API handle utilities for wrapping/unwrapping mailcore C++ pointers.
//  Follows the pattern from src/java/JavaHandle.h but adapted for N-API.
//

#ifndef NAPI_HANDLE_H
#define NAPI_HANDLE_H

#include <napi.h>
#include <MailCore/MCAutoreleasePool.h>

// RAII wrapper for mailcore::AutoreleasePool — use at the top of each N-API function
class NapiAutoreleasePool {
public:
    NapiAutoreleasePool() : pool() {}
    ~NapiAutoreleasePool() = default;
private:
    mailcore::AutoreleasePool pool;
};

// Template helper: wrap a mailcore::Object* into a Napi::External with release destructor
template<typename T>
Napi::External<T> WrapMailcoreObject(Napi::Env env, T* obj) {
    if (obj) obj->retain();
    return Napi::External<T>::New(env, obj, [](Napi::Env, T* ptr) {
        if (ptr) ptr->release();
    });
}

// Template helper: unwrap a mailcore::Object* from a Napi::External
template<typename T>
T* UnwrapMailcoreObject(Napi::Value value) {
    return value.As<Napi::External<T>>().Data();
}

#endif /* NAPI_HANDLE_H */
