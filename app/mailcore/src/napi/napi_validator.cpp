//
//  napi_validator.cpp
//  mailcore-napi
//
//  N-API binding for AccountValidator — async account validation from Node.js.
//

#include <napi.h>
#include "napi_handle.h"
#include "napi_types.h"
#include <MailCore/MCAccountValidator.h>
#include <MailCore/MCMailProvidersManager.h>

using namespace mailcore;

class ValidateAccountWorker : public Napi::AsyncWorker {
public:
    ValidateAccountWorker(Napi::Env env, Napi::Promise::Deferred deferred,
                          std::string email, std::string password, std::string oauth2Token,
                          std::string imapHostname, int imapPort,
                          std::string smtpHostname, int smtpPort)
        : Napi::AsyncWorker(env),
          deferred_(deferred),
          email_(email), password_(password), oauth2Token_(oauth2Token),
          imapHostname_(imapHostname), imapPort_(imapPort),
          smtpHostname_(smtpHostname), smtpPort_(smtpPort),
          success_(false) {}

    void Execute() override {
        AutoreleasePool pool;

        AccountValidator* validator = new AccountValidator();
        validator->setEmail(String::stringWithUTF8Characters(email_.c_str()));

        if (!password_.empty()) {
            validator->setPassword(String::stringWithUTF8Characters(password_.c_str()));
        }
        if (!oauth2Token_.empty()) {
            validator->setOAuth2Token(String::stringWithUTF8Characters(oauth2Token_.c_str()));
        }
        if (!imapHostname_.empty()) {
            validator->setImapServer(
                NetService::serviceWithInfo(
                    String::stringWithUTF8Characters(imapHostname_.c_str()),
                    imapPort_,
                    ConnectionTypeTLS
                )
            );
        }
        if (!smtpHostname_.empty()) {
            validator->setSmtpServer(
                NetService::serviceWithInfo(
                    String::stringWithUTF8Characters(smtpHostname_.c_str()),
                    smtpPort_,
                    ConnectionTypeTLS
                )
            );
        }

        validator->start();

        // Wait for validation to complete
        while (!validator->isFinished()) {
            std::this_thread::sleep_for(std::chrono::milliseconds(50));
        }

        ErrorCode err = validator->error();
        success_ = (err == ErrorNone);

        if (!success_) {
            String* errDesc = ErrorMessage::messageForError(err);
            if (errDesc) {
                errorMessage_ = errDesc->UTF8Characters();
            } else {
                errorMessage_ = "Validation failed with error code " + std::to_string(err);
            }
        }

        // Capture results
        if (validator->identifier()) {
            identifier_ = validator->identifier()->UTF8Characters();
        }
        if (validator->imapServer()) {
            imapResultHost_ = validator->imapServer()->hostname() ?
                validator->imapServer()->hostname()->UTF8Characters() : "";
            imapResultPort_ = validator->imapServer()->port();
        }
        if (validator->smtpServer()) {
            smtpResultHost_ = validator->smtpServer()->hostname() ?
                validator->smtpServer()->hostname()->UTF8Characters() : "";
            smtpResultPort_ = validator->smtpServer()->port();
        }

        validator->release();
    }

    void OnOK() override {
        Napi::Env env = Env();
        Napi::Object result = Napi::Object::New(env);
        result.Set("success", Napi::Boolean::New(env, success_));

        if (!success_) {
            result.Set("error", Napi::String::New(env, errorMessage_));
        }

        if (!identifier_.empty()) {
            result.Set("identifier", Napi::String::New(env, identifier_));
        }

        Napi::Object imapServer = Napi::Object::New(env);
        imapServer.Set("hostname", Napi::String::New(env, imapResultHost_));
        imapServer.Set("port", Napi::Number::New(env, imapResultPort_));
        result.Set("imapServer", imapServer);

        Napi::Object smtpServer = Napi::Object::New(env);
        smtpServer.Set("hostname", Napi::String::New(env, smtpResultHost_));
        smtpServer.Set("port", Napi::Number::New(env, smtpResultPort_));
        result.Set("smtpServer", smtpServer);

        deferred_.Resolve(result);
    }

    void OnError(const Napi::Error& err) override {
        deferred_.Reject(err.Value());
    }

private:
    Napi::Promise::Deferred deferred_;
    std::string email_, password_, oauth2Token_;
    std::string imapHostname_, smtpHostname_;
    int imapPort_, smtpPort_;
    bool success_;
    std::string errorMessage_;
    std::string identifier_;
    std::string imapResultHost_, smtpResultHost_;
    int imapResultPort_ = 0, smtpResultPort_ = 0;
};

// validateAccount(opts): Promise<AccountValidationResult>
Napi::Value ValidateAccount(const Napi::CallbackInfo& info) {
    Napi::Env env = info.Env();

    if (info.Length() < 1 || !info[0].IsObject()) {
        Napi::TypeError::New(env, "Expected an options object").ThrowAsJavaScriptException();
        return env.Undefined();
    }

    Napi::Object opts = info[0].As<Napi::Object>();

    std::string email = opts.Has("email") ? opts.Get("email").As<Napi::String>().Utf8Value() : "";
    std::string password = opts.Has("password") ? opts.Get("password").As<Napi::String>().Utf8Value() : "";
    std::string oauth2Token = opts.Has("oauth2Token") ? opts.Get("oauth2Token").As<Napi::String>().Utf8Value() : "";
    std::string imapHostname = opts.Has("imapHostname") ? opts.Get("imapHostname").As<Napi::String>().Utf8Value() : "";
    int imapPort = opts.Has("imapPort") ? opts.Get("imapPort").As<Napi::Number>().Int32Value() : 993;
    std::string smtpHostname = opts.Has("smtpHostname") ? opts.Get("smtpHostname").As<Napi::String>().Utf8Value() : "";
    int smtpPort = opts.Has("smtpPort") ? opts.Get("smtpPort").As<Napi::Number>().Int32Value() : 587;

    auto deferred = Napi::Promise::Deferred::New(env);
    auto* worker = new ValidateAccountWorker(
        env, deferred,
        email, password, oauth2Token,
        imapHostname, imapPort,
        smtpHostname, smtpPort
    );
    worker->Queue();

    return deferred.Promise();
}

void InitValidator(Napi::Env env, Napi::Object exports) {
    exports.Set("validateAccount", Napi::Function::New(env, ValidateAccount));
}
