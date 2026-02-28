//
//  napi_types.h
//  mailcore-napi
//
//  Type conversion utilities between mailcore types and N-API values.
//  Follows the pattern from src/java/TypesUtils.h.
//

#ifndef NAPI_TYPES_H
#define NAPI_TYPES_H

#include <napi.h>
#include <MailCore/MCString.h>
#include <MailCore/MCArray.h>
#include <MailCore/MCHashMap.h>

namespace NapiTypes {

// mailcore::String* -> Napi::String (returns empty string if null)
Napi::String MCStringToNapi(Napi::Env env, mailcore::String* str);

// Napi::String -> mailcore::String* (caller must release or use within pool)
mailcore::String* NapiToMCString(Napi::Value value);

// mailcore::Array* -> Napi::Array of strings
Napi::Array MCStringArrayToNapi(Napi::Env env, mailcore::Array* arr);

// mailcore::HashMap* -> Napi::Object
Napi::Object MCHashMapToNapi(Napi::Env env, mailcore::HashMap* map);

// Convert ConnectionType enum to string
Napi::String ConnectionTypeToNapi(Napi::Env env, int connectionType);

// Convert string to ConnectionType enum
int NapiToConnectionType(const std::string& str);

} // namespace NapiTypes

#endif /* NAPI_TYPES_H */
