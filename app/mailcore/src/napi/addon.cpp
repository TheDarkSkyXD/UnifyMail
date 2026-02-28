//
//  addon.cpp
//  mailcore-napi
//
//  N-API module entry point. Registers all bindings and initializes
//  the mailcore providers database from the bundled providers.json.
//

#include <napi.h>
#include <MailCore/MCAutoreleasePool.h>
#include <MailCore/MCMailProvidersManager.h>
#include <MailCore/MCString.h>

#ifdef _WIN32
#include <windows.h>
#endif

// Forward declarations for sub-module initializers
void InitProvider(Napi::Env env, Napi::Object exports);
void InitValidator(Napi::Env env, Napi::Object exports);
void InitIMAP(Napi::Env env, Napi::Object exports);
void InitSMTP(Napi::Env env, Napi::Object exports);

// Resolve the path to providers.json relative to this addon's DLL location
static std::string GetProvidersJsonPath() {
#ifdef _WIN32
    char modulePath[MAX_PATH];
    HMODULE hModule = NULL;

    // Get the path to this DLL/node file
    GetModuleHandleExA(
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
        (LPCSTR)&GetProvidersJsonPath,
        &hModule
    );
    GetModuleFileNameA(hModule, modulePath, MAX_PATH);

    std::string path(modulePath);
    // Navigate from build/Release/mailcore_napi.node up to the mailcore root
    size_t pos = path.rfind('\\');
    if (pos != std::string::npos) path = path.substr(0, pos); // build/Release
    pos = path.rfind('\\');
    if (pos != std::string::npos) path = path.substr(0, pos); // build
    pos = path.rfind('\\');
    if (pos != std::string::npos) path = path.substr(0, pos); // mailcore root

    return path + "\\resources\\providers.json";
#else
    // On other platforms, use a relative path from the process working directory
    return "app/mailcore/resources/providers.json";
#endif
}

Napi::Object Init(Napi::Env env, Napi::Object exports) {
    mailcore::AutoreleasePool pool;

    // Auto-load providers.json from the addon's directory
    std::string providersPath = GetProvidersJsonPath();
    mailcore::String* mcPath = mailcore::String::stringWithUTF8Characters(providersPath.c_str());
    mailcore::MailProvidersManager::sharedManager()->registerProvidersWithFilename(mcPath);

    // Initialize sub-modules
    InitProvider(env, exports);
    InitValidator(env, exports);
    InitIMAP(env, exports);
    InitSMTP(env, exports);

    return exports;
}

NODE_API_MODULE(mailcore_napi, Init)
