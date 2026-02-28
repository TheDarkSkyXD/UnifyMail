{
  "targets": [{
    "target_name": "mailcore_napi",
    "sources": [
      "src/napi/addon.cpp",
      "src/napi/napi_types.cpp",
      "src/napi/napi_provider.cpp",
      "src/napi/napi_validator.cpp",
      "src/napi/napi_imap.cpp",
      "src/napi/napi_smtp.cpp"
    ],
    "include_dirs": [
      "<!@(node -p \"require('node-addon-api').include\")",
      "build-windows/include",
      "src/core/basetypes",
      "src/core/abstract",
      "src/core/provider",
      "src/core/imap",
      "src/core/smtp",
      "src/core/pop",
      "src/core/nntp",
      "src/core/rfc822",
      "src/core/renderer",
      "src/core/security",
      "src/core/zip"
    ],
    "defines": [
      "NAPI_VERSION=8",
      "NAPI_DISABLE_CPP_EXCEPTIONS"
    ],
    "conditions": [
      ["OS=='win'", {
        "include_dirs": [
          "<!(echo %VCPKG_ROOT%)/installed/x64-windows/include",
          "<!(echo %VCPKG_ROOT%)/installed/x64-windows/include/libxml2",
          "../mailsync/Vendor/libetpan/build-windows/include"
        ],
        "libraries": [
          "<(module_root_dir)/build-windows/mailcore2/mailcore2/x64/Release/mailcore2.lib",
          "<(module_root_dir)/../mailsync/Vendor/libetpan/build-windows/libetpan/x64/Release/libetpan.lib",
          "-lCrypt32",
          "-lUser32",
          "-lWs2_32",
          "-lkernel32"
        ],
        "msvs_settings": {
          "VCCLCompilerTool": {
            "RuntimeLibrary": 2,
            "AdditionalOptions": ["/std:c++17"]
          }
        },
        "copies": [{
          "destination": "<(module_root_dir)/build/Release",
          "files": [
            "<(module_root_dir)/build-windows/mailcore2/mailcore2/x64/Release/mailcore2.dll",
            "<(module_root_dir)/../mailsync/Vendor/libetpan/build-windows/libetpan/x64/Release/libetpan.dll"
          ]
        }]
      }]
    ]
  }]
}
