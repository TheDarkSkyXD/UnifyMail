//
//  napi_imap.cpp
//  mailcore-napi
//
//  N-API binding for quick IMAP connection testing from Node.js.
//

#include <napi.h>
#include "napi_handle.h"
#include "napi_types.h"
#include <MailCore/MCIMAPSession.h>
#include <MailCore/MCIMAPCapabilityOperation.h>

using namespace mailcore;

class TestIMAPWorker : public Napi::AsyncWorker {
public:
    TestIMAPWorker(Napi::Env env, Napi::Promise::Deferred deferred,
                   std::string hostname, int port, int connectionType,
                   std::string username, std::string password, std::string oauth2Token)
        : Napi::AsyncWorker(env),
          deferred_(deferred),
          hostname_(hostname), port_(port), connectionType_(connectionType),
          username_(username), password_(password), oauth2Token_(oauth2Token),
          success_(false) {}

    void Execute() override {
        AutoreleasePool pool;

        IMAPSession* session = new IMAPSession();
        session->setHostname(String::stringWithUTF8Characters(hostname_.c_str()));
        session->setPort(port_);
        session->setConnectionType((ConnectionType)connectionType_);

        if (!username_.empty()) {
            session->setUsername(String::stringWithUTF8Characters(username_.c_str()));
        }
        if (!password_.empty()) {
            session->setPassword(String::stringWithUTF8Characters(password_.c_str()));
        }
        if (!oauth2Token_.empty()) {
            session->setOAuth2Token(String::stringWithUTF8Characters(oauth2Token_.c_str()));
            session->setAuthType(AuthTypeXOAuth2);
        }

        ErrorCode err = ErrorNone;
        session->connectIfNeeded(&err);

        if (err == ErrorNone) {
            success_ = true;

            // Check capabilities
            IMAPCapability caps = session->capability();
            if (caps & IMAPCapabilityIdle) capabilities_.push_back("idle");
            if (caps & IMAPCapabilityCondstore) capabilities_.push_back("condstore");
            if (caps & IMAPCapabilityQResync) capabilities_.push_back("qresync");
            if (caps & IMAPCapabilityCompressDeflate) capabilities_.push_back("compress");
            if (caps & IMAPCapabilityNamespace) capabilities_.push_back("namespace");
            if (caps & IMAPCapabilityXOAuth2) capabilities_.push_back("xoauth2");
            if (caps & IMAPCapabilityGmail) capabilities_.push_back("gmail");
        } else {
            success_ = false;
            String* errDesc = ErrorMessage::messageForError(err);
            if (errDesc) {
                errorMessage_ = errDesc->UTF8Characters();
            } else {
                errorMessage_ = "IMAP connection failed with error code " + std::to_string(err);
            }
        }

        session->disconnect();
        session->release();
    }

    void OnOK() override {
        Napi::Env env = Env();
        Napi::Object result = Napi::Object::New(env);
        result.Set("success", Napi::Boolean::New(env, success_));

        if (!success_) {
            result.Set("error", Napi::String::New(env, errorMessage_));
        }

        Napi::Array caps = Napi::Array::New(env, capabilities_.size());
        for (size_t i = 0; i < capabilities_.size(); i++) {
            caps.Set(i, Napi::String::New(env, capabilities_[i]));
        }
        result.Set("capabilities", caps);

        deferred_.Resolve(result);
    }

    void OnError(const Napi::Error& err) override {
        deferred_.Reject(err.Value());
    }

private:
    Napi::Promise::Deferred deferred_;
    std::string hostname_, username_, password_, oauth2Token_;
    int port_, connectionType_;
    bool success_;
    std::string errorMessage_;
    std::vector<std::string> capabilities_;
};

// testIMAPConnection(opts): Promise<{success, error?, capabilities?}>
Napi::Value TestIMAPConnection(const Napi::CallbackInfo& info) {
    Napi::Env env = info.Env();

    if (info.Length() < 1 || !info[0].IsObject()) {
        Napi::TypeError::New(env, "Expected an options object").ThrowAsJavaScriptException();
        return env.Undefined();
    }

    Napi::Object opts = info[0].As<Napi::Object>();

    std::string hostname = opts.Get("hostname").As<Napi::String>().Utf8Value();
    int port = opts.Get("port").As<Napi::Number>().Int32Value();
    std::string connTypeStr = opts.Has("connectionType") ?
        opts.Get("connectionType").As<Napi::String>().Utf8Value() : "tls";
    int connectionType = NapiTypes::NapiToConnectionType(connTypeStr);

    std::string username = opts.Has("username") ? opts.Get("username").As<Napi::String>().Utf8Value() : "";
    std::string password = opts.Has("password") ? opts.Get("password").As<Napi::String>().Utf8Value() : "";
    std::string oauth2Token = opts.Has("oauth2Token") ? opts.Get("oauth2Token").As<Napi::String>().Utf8Value() : "";

    auto deferred = Napi::Promise::Deferred::New(env);
    auto* worker = new TestIMAPWorker(env, deferred,
        hostname, port, connectionType,
        username, password, oauth2Token);
    worker->Queue();

    return deferred.Promise();
}

void InitIMAP(Napi::Env env, Napi::Object exports) {
    exports.Set("testIMAPConnection", Napi::Function::New(env, TestIMAPConnection));
}
