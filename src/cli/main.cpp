// lumen — the agent-facing CLI.
//
// Speaks JSON lines to the running application over a unix socket. There is no
// network surface to reach; the socket lives in the user's runtime directory.

#include <QCommandLineParser>
#include <QCoreApplication>
#include <QDeadlineTimer>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLocalSocket>
#include <QStandardPaths>
#include <QTextStream>

namespace {

QString socketPath() {
    // Inside a session the runtime directory is the session's own, so the
    // daemon's published path is the only one that reaches it.
    const QByteArray published = qgetenv("LUMEN_AGENT_SOCKET");
    if (!published.isEmpty()) {
        return QString::fromLocal8Bit(published);
    }
    QString runtime = QStandardPaths::writableLocation(QStandardPaths::RuntimeLocation);
    if (runtime.isEmpty()) {
        runtime = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation);
    }
    return runtime + QStringLiteral("/lumen-agent.sock");
}

/// Send one request and read its reply into `reply`. Returns the exit status:
/// 0 on a reply without an error, 1 on an error reply, 2 when the daemon is
/// not running, 3 when it does not answer.
int exchange(const QJsonObject& request, QJsonObject* reply) {
    QLocalSocket socket;
    socket.connectToServer(socketPath());
    if (!socket.waitForConnected(3000)) {
        QTextStream(stderr) << "lumen: not running (no socket at " << socketPath() << ")\n";
        return 2;
    }
    socket.write(QJsonDocument(request).toJson(QJsonDocument::Compact) + "\n");
    socket.flush();
    // The reply is one newline-terminated line, and a large one — a frame, a
    // note's image — arrives over many reads. Parsing the first read alone
    // would see a truncated document.
    QByteArray response;
    const QDeadlineTimer deadline(10000);
    while (!response.contains('\n')) {
        if (!socket.waitForReadyRead(int(deadline.remainingTime()))) {
            QTextStream(stderr) << "lumen: no response\n";
            return 3;
        }
        response += socket.readAll();
    }
    socket.disconnectFromServer();
    *reply = QJsonDocument::fromJson(response.left(response.indexOf('\n'))).object();
    if (reply->contains(QStringLiteral("error"))) {
        QTextStream(stderr) << "lumen: " << (*reply)[QStringLiteral("error")].toString() << "\n";
        return 1;
    }
    return 0;
}

void print(const QJsonObject& object) {
    QTextStream(stdout) << QString::fromUtf8(QJsonDocument(object).toJson(QJsonDocument::Indented));
}

int send(const QJsonObject& request) {
    QJsonObject reply;
    const int status = exchange(request, &reply);
    if (status == 0) {
        print(reply);
    }
    return status;
}

/// Write a session's composited frame to `path` as a PNG. The image goes to
/// the file, never to stdout; what is printed is where it went and its size.
int saveFrame(const QJsonObject& request, const QString& path) {
    QJsonObject reply;
    const int status = exchange(request, &reply);
    if (status != 0) {
        return status;
    }
    const QByteArray png =
        QByteArray::fromBase64(reply[QStringLiteral("image")].toString().toLatin1());
    QFile file(path);
    if (png.isEmpty() || !file.open(QIODevice::WriteOnly) || file.write(png) != png.size()) {
        QTextStream(stderr) << "lumen: could not write the frame to " << path << "\n";
        return 1;
    }
    file.close();
    print(QJsonObject{{QStringLiteral("path"), QFileInfo(file).absoluteFilePath()},
                      {QStringLiteral("width"), reply[QStringLiteral("width")]},
                      {QStringLiteral("height"), reply[QStringLiteral("height")]}});
    return 0;
}

} // namespace

int main(int argc, char** argv) {
    QCoreApplication app(argc, argv);
    QCommandLineParser parser;
    parser.setApplicationDescription(QStringLiteral("Drive Lumen sessions from an agent"));
    parser.addHelpOption();
    parser.addPositionalArgument(
        QStringLiteral("command"),
        QStringLiteral("ensure|status|stop|accessibility|tree|click|type|frame|feedback|ack"));
    parser.addPositionalArgument(QStringLiteral("name"), QStringLiteral("session name"));
    parser.addPositionalArgument(QStringLiteral("argument"),
                                 QStringLiteral("command, text, output PNG, or reference"));
    QCommandLineOption ownerOption(QStringLiteral("owner"), QStringLiteral("agent label"),
                                   QStringLiteral("owner"));
    QCommandLineOption consumeOption(QStringLiteral("consume"),
                                     QStringLiteral("acknowledge the notes after reading"));
    QCommandLineOption xOption(QStringLiteral("x"), QStringLiteral("x coordinate"),
                               QStringLiteral("x"));
    QCommandLineOption yOption(QStringLiteral("y"), QStringLiteral("y coordinate"),
                               QStringLiteral("y"));
    QCommandLineOption idOption(QStringLiteral("id"), QStringLiteral("note id"),
                                QStringLiteral("id"));
    QCommandLineOption allOption(QStringLiteral("all"),
                                 QStringLiteral("tree: include hidden items and windows"));
    parser.addOptions({ownerOption, consumeOption, xOption, yOption, idOption, allOption});
    parser.process(app);

    const QStringList args = parser.positionalArguments();
    if (args.isEmpty()) {
        parser.showHelp(2);
    }
    const QString command = args.at(0);
    const QString name = args.value(1);
    const QString argument = args.value(2);

    QJsonObject request;
    request[QStringLiteral("cmd")] = command;
    if (!name.isEmpty()) {
        request[QStringLiteral("name")] = name;
    }
    if (parser.isSet(ownerOption)) {
        request[QStringLiteral("owner")] = parser.value(ownerOption);
    }
    if (command == QLatin1String("ensure")) {
        request[QStringLiteral("command")] = argument;
    } else if (command == QLatin1String("type")) {
        request[QStringLiteral("text")] = argument;
    } else if (command == QLatin1String("click")) {
        // Coordinates are accepted positionally after the name, so the common
        // form `lumen click <session> <x> <y>` works without the options.
        request[QStringLiteral("x")] =
            parser.isSet(xOption) ? parser.value(xOption).toDouble() : args.value(2).toDouble();
        request[QStringLiteral("y")] =
            parser.isSet(yOption) ? parser.value(yOption).toDouble() : args.value(3).toDouble();
    } else if (command == QLatin1String("feedback")) {
        request[QStringLiteral("consume")] = parser.isSet(consumeOption);
    } else if (command == QLatin1String("ack")) {
        request[QStringLiteral("id")] = parser.value(idOption).toLongLong();
    } else if (command == QLatin1String("tree")) {
        request[QStringLiteral("all")] = parser.isSet(allOption);
    } else if (command == QLatin1String("frame")) {
        if (name.isEmpty() || argument.isEmpty()) {
            QTextStream(stderr) << "usage: lumen-cli frame <name> <output.png>\n";
            return 2;
        }
        return saveFrame(request, argument);
    }
    return send(request);
}
