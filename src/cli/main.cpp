// lumen — the agent-facing CLI.
//
// Speaks JSON lines to the running application over a unix socket. There is no
// network surface to reach; the socket lives in the user's runtime directory.

#include <QCoreApplication>
#include <QCommandLineParser>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLocalSocket>
#include <QStandardPaths>
#include <QTextStream>

namespace {

QString socketPath() {
    QString runtime = QStandardPaths::writableLocation(QStandardPaths::RuntimeLocation);
    if (runtime.isEmpty()) {
        runtime = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation);
    }
    return runtime + QStringLiteral("/lumen-agent.sock");
}

int send(const QJsonObject &request) {
    QLocalSocket socket;
    socket.connectToServer(socketPath());
    if (!socket.waitForConnected(3000)) {
        QTextStream(stderr) << "lumen: not running (no socket at " << socketPath() << ")\n";
        return 2;
    }
    socket.write(QJsonDocument(request).toJson(QJsonDocument::Compact) + "\n");
    socket.flush();
    if (!socket.waitForReadyRead(10000)) {
        QTextStream(stderr) << "lumen: no response\n";
        return 3;
    }
    const QByteArray response = socket.readAll().trimmed();
    socket.disconnectFromServer();
    const QJsonObject object = QJsonDocument::fromJson(response).object();
    if (object.contains(QStringLiteral("error"))) {
        QTextStream(stderr) << "lumen: " << object[QStringLiteral("error")].toString() << "\n";
        return 1;
    }
    QTextStream(stdout) << QString::fromUtf8(
        QJsonDocument(object).toJson(QJsonDocument::Indented));
    return 0;
}

} // namespace

int main(int argc, char **argv) {
    QCoreApplication app(argc, argv);
    QCommandLineParser parser;
    parser.setApplicationDescription(
        QStringLiteral("Drive Lumen sessions from an agent"));
    parser.addHelpOption();
    parser.addPositionalArgument(QStringLiteral("command"),
                                 QStringLiteral("ensure|status|stop|accessibility|click|type|feedback|ack"));
    parser.addPositionalArgument(QStringLiteral("name"), QStringLiteral("session name"));
    parser.addPositionalArgument(QStringLiteral("argument"),
                                 QStringLiteral("command, text, or reference"));
    QCommandLineOption ownerOption(QStringLiteral("owner"),
                                   QStringLiteral("agent label"), QStringLiteral("owner"));
    QCommandLineOption consumeOption(QStringLiteral("consume"),
                                     QStringLiteral("acknowledge the notes after reading"));
    QCommandLineOption xOption(QStringLiteral("x"), QStringLiteral("x coordinate"), QStringLiteral("x"));
    QCommandLineOption yOption(QStringLiteral("y"), QStringLiteral("y coordinate"), QStringLiteral("y"));
    QCommandLineOption idOption(QStringLiteral("id"), QStringLiteral("note id"), QStringLiteral("id"));
    parser.addOptions({ownerOption, consumeOption, xOption, yOption, idOption});
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
        request[QStringLiteral("x")] = parser.value(xOption).toDouble();
        request[QStringLiteral("y")] = parser.value(yOption).toDouble();
    } else if (command == QLatin1String("feedback")) {
        request[QStringLiteral("consume")] = parser.isSet(consumeOption);
    } else if (command == QLatin1String("ack")) {
        request[QStringLiteral("id")] = parser.value(idOption).toLongLong();
    }
    return send(request);
}
