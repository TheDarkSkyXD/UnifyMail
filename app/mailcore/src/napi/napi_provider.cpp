//
//  napi_provider.cpp
//  mailcore-napi
//
//  N-API binding for MailProvidersManager — exposes provider detection to Node.js.
//

#include <napi.h>
#include "napi_handle.h"
#include "napi_types.h"
#include <MailCore/MCMailProvidersManager.h>
#include <MailCore/MCMailProvider.h>
#include <MailCore/MCNetService.h>

using namespace mailcore;

// Helper: serialize a NetService to a JS object
static Napi::Object SerializeNetService(Napi::Env env, NetService* svc) {
    Napi::Object obj = Napi::Object::New(env);
    if (!svc) return obj;

    obj.Set("hostname", NapiTypes::MCStringToNapi(env, svc->hostname()));
    obj.Set("port", Napi::Number::New(env, svc->port()));
    obj.Set("connectionType", NapiTypes::ConnectionTypeToNapi(env, svc->connectionType()));
    return obj;
}

// Helper: serialize an array of NetService* to a JS array
static Napi::Array SerializeNetServiceArray(Napi::Env env, Array* services) {
    Napi::Array arr = Napi::Array::New(env);
    if (!services) return arr;

    for (unsigned int i = 0; i < services->count(); i++) {
        NetService* svc = (NetService*) services->objectAtIndex(i);
        arr.Set(i, SerializeNetService(env, svc));
    }
    return arr;
}

// providerForEmail(email: string): MailProviderInfo | null
Napi::Value ProviderForEmail(const Napi::CallbackInfo& info) {
    Napi::Env env = info.Env();
    NapiAutoreleasePool pool;

    if (info.Length() < 1 || !info[0].IsString()) {
        Napi::TypeError::New(env, "Expected a string argument (email)").ThrowAsJavaScriptException();
        return env.Null();
    }

    String* email = NapiTypes::NapiToMCString(info[0]);
    MailProvider* provider = MailProvidersManager::sharedManager()->providerForEmail(email);

    if (!provider) {
        return env.Null();
    }

    Napi::Object result = Napi::Object::New(env);
    result.Set("identifier", NapiTypes::MCStringToNapi(env, provider->identifier()));

    // Serialize IMAP servers
    Napi::Object servers = Napi::Object::New(env);
    servers.Set("imap", SerializeNetServiceArray(env, provider->imapServices()));
    servers.Set("smtp", SerializeNetServiceArray(env, provider->smtpServices()));
    servers.Set("pop", SerializeNetServiceArray(env, provider->popServices()));
    result.Set("servers", servers);

    // Domain match patterns
    result.Set("domainMatch", NapiTypes::MCStringArrayToNapi(env, provider->domainMatch()));
    result.Set("mxMatch", NapiTypes::MCStringArrayToNapi(env, provider->mxMatch()));

    return result;
}

// registerProviders(jsonPath: string): void
// Loads providers.json from an absolute path
Napi::Value RegisterProviders(const Napi::CallbackInfo& info) {
    Napi::Env env = info.Env();
    NapiAutoreleasePool pool;

    if (info.Length() < 1 || !info[0].IsString()) {
        Napi::TypeError::New(env, "Expected a string argument (path to providers.json)").ThrowAsJavaScriptException();
        return env.Undefined();
    }

    String* path = NapiTypes::NapiToMCString(info[0]);
    MailProvidersManager::sharedManager()->registerProvidersWithFilename(path);
    return env.Undefined();
}

void InitProvider(Napi::Env env, Napi::Object exports) {
    exports.Set("providerForEmail", Napi::Function::New(env, ProviderForEmail));
    exports.Set("registerProviders", Napi::Function::New(env, RegisterProviders));
}
