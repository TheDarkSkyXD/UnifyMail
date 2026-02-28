//
//  napi_types.cpp
//  mailcore-napi
//
//  Type conversion utilities between mailcore types and N-API values.
//

#include "napi_types.h"
#include <MailCore/MCValue.h>
#include <MailCore/MCMessageConstants.h>

namespace NapiTypes {

Napi::String MCStringToNapi(Napi::Env env, mailcore::String* str) {
    if (str == nullptr) {
        return Napi::String::New(env, "");
    }
    const char* utf8 = str->UTF8Characters();
    if (utf8 == nullptr) {
        return Napi::String::New(env, "");
    }
    return Napi::String::New(env, utf8);
}

mailcore::String* NapiToMCString(Napi::Value value) {
    if (value.IsNull() || value.IsUndefined()) {
        return nullptr;
    }
    std::string str = value.As<Napi::String>().Utf8Value();
    return mailcore::String::stringWithUTF8Characters(str.c_str());
}

Napi::Array MCStringArrayToNapi(Napi::Env env, mailcore::Array* arr) {
    Napi::Array result = Napi::Array::New(env);
    if (arr == nullptr) {
        return result;
    }
    for (unsigned int i = 0; i < arr->count(); i++) {
        mailcore::String* item = (mailcore::String*) arr->objectAtIndex(i);
        result.Set(i, MCStringToNapi(env, item));
    }
    return result;
}

Napi::Object MCHashMapToNapi(Napi::Env env, mailcore::HashMap* map) {
    Napi::Object result = Napi::Object::New(env);
    if (map == nullptr) {
        return result;
    }
    mailcore::Array* keys = map->allKeys();
    if (keys == nullptr) {
        return result;
    }
    for (unsigned int i = 0; i < keys->count(); i++) {
        mailcore::String* key = (mailcore::String*) keys->objectAtIndex(i);
        mailcore::Object* val = map->objectForKey(key);
        if (val == nullptr) continue;

        // Try to convert value based on type
        mailcore::String* strVal = dynamic_cast<mailcore::String*>(val);
        if (strVal) {
            result.Set(MCStringToNapi(env, key), MCStringToNapi(env, strVal));
            continue;
        }
        mailcore::Value* numVal = dynamic_cast<mailcore::Value*>(val);
        if (numVal) {
            result.Set(MCStringToNapi(env, key), Napi::Number::New(env, numVal->intValue()));
            continue;
        }
        // Fallback: use description string
        result.Set(MCStringToNapi(env, key), MCStringToNapi(env, val->description()));
    }
    return result;
}

Napi::String ConnectionTypeToNapi(Napi::Env env, int connectionType) {
    switch (connectionType) {
        case mailcore::ConnectionTypeTLS:
            return Napi::String::New(env, "tls");
        case mailcore::ConnectionTypeStartTLS:
            return Napi::String::New(env, "starttls");
        case mailcore::ConnectionTypeClear:
        default:
            return Napi::String::New(env, "clear");
    }
}

int NapiToConnectionType(const std::string& str) {
    if (str == "tls") return mailcore::ConnectionTypeTLS;
    if (str == "starttls") return mailcore::ConnectionTypeStartTLS;
    return mailcore::ConnectionTypeClear;
}

} // namespace NapiTypes
