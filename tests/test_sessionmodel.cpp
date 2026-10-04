#include "config.h"
#include "sessionmanager.h"

#include <QTest>

// The session model's validation and limit behavior. Actually starting a client
// needs a compositor and a display, which is the spike's job, not a unit test's.
class TestSessionModel : public QObject {
    Q_OBJECT

  private slots:
    void refuses_an_invalid_name_before_starting_anything() {
        Config config;
        SessionManager manager(&config, nullptr);
        QString error;
        const QString created =
            manager.create(QStringLiteral("../escape"), QStringLiteral("/bin/true"), true,
                           QStringLiteral("test"), &error);
        QVERIFY(created.isEmpty());
        QCOMPARE(error, QStringLiteral("invalid session name"));
        QCOMPARE(manager.count(), 0);
    }

    void refuses_a_command_that_is_not_an_executable() {
        Config config;
        SessionManager manager(&config, nullptr);
        QString error;
        const QString created = manager.create(QStringLiteral("alice"), QStringLiteral("ls"), true,
                                               QStringLiteral("test"), &error);
        QVERIFY(created.isEmpty());
        QVERIFY(error.contains(QStringLiteral("absolute path")));
        QCOMPARE(manager.count(), 0);
    }

    void reports_an_unknown_session_as_absent() {
        Config config;
        SessionManager manager(&config, nullptr);
        QVERIFY(manager.session(QStringLiteral("nobody")) == nullptr);
        QVERIFY(!manager.stop(QStringLiteral("nobody")));
        QVERIFY(manager.processIdFor(QStringLiteral("nobody")).isEmpty());
    }
};

QTEST_MAIN(TestSessionModel)
#include "test_sessionmodel.moc"
