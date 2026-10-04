#include "feedbackstore.h"

#include <QJsonArray>
#include <QJsonObject>
#include <QTest>
#include <QTemporaryDir>

class TestFeedbackStore : public QObject {
    Q_OBJECT

private slots:
    void consume_returns_exactly_what_it_acknowledges() {
        QTemporaryDir dir;
        FeedbackStore store(dir.path() + QStringLiteral("/f.db"));
        store.add(QStringLiteral("s"), QStringLiteral("human"), QStringLiteral("first"), {});
        store.add(QStringLiteral("s"), QStringLiteral("human"), QStringLiteral("second"), {});

        const QJsonArray taken = store.consume(QStringLiteral("s"));
        QCOMPARE(taken.size(), 2);
        QCOMPARE(taken[0].toObject()[QStringLiteral("comment")].toString(),
                 QStringLiteral("first"));
        QVERIFY(store.list(QStringLiteral("s"), true).isEmpty());

        store.add(QStringLiteral("s"), QStringLiteral("human"), QStringLiteral("after"), {});
        const QJsonArray next = store.consume(QStringLiteral("s"));
        QCOMPARE(next.size(), 1);
        QCOMPARE(next[0].toObject()[QStringLiteral("comment")].toString(),
                 QStringLiteral("after"));
    }

    void screenshots_round_trip_and_are_listed_by_presence() {
        QTemporaryDir dir;
        FeedbackStore store(dir.path() + QStringLiteral("/f.db"));
        const QByteArray png = "\x89PNG\r\n\x1a\nnot-really-a-png";
        const QJsonObject withImage =
            store.add(QStringLiteral("s"), QStringLiteral("human"), QStringLiteral("with image"), png);
        store.add(QStringLiteral("s"), QStringLiteral("human"), QStringLiteral("no image"), {});

        QCOMPARE(store.screenshot(QStringLiteral("s"),
                                  withImage[QStringLiteral("id")].toInteger()),
                 png);

        const QJsonArray listed = store.list(QStringLiteral("s"), false);
        QCOMPARE(listed.size(), 2);
        QVERIFY(listed[0].toObject()[QStringLiteral("screenshot")].toBool());
        QVERIFY(!listed[1].toObject()[QStringLiteral("screenshot")].toBool());
        QVERIFY(store.screenshot(QStringLiteral("s"), 404).isEmpty());
    }

    void acknowledging_reports_whether_the_note_exists() {
        QTemporaryDir dir;
        FeedbackStore store(dir.path() + QStringLiteral("/f.db"));
        QVERIFY(!store.ack(QStringLiteral("s"), 404));
        const QJsonObject note =
            store.add(QStringLiteral("s"), QStringLiteral("human"), QStringLiteral("note"), {});
        const qint64 id = note[QStringLiteral("id")].toInteger();
        QVERIFY(store.ack(QStringLiteral("s"), id));
        // Acknowledging again is idempotent: the note still exists.
        QVERIFY(store.ack(QStringLiteral("s"), id));
        QVERIFY(store.list(QStringLiteral("s"), true).isEmpty());
    }

    void notes_are_scoped_to_their_session() {
        QTemporaryDir dir;
        FeedbackStore store(dir.path() + QStringLiteral("/f.db"));
        store.add(QStringLiteral("a"), QStringLiteral("human"), QStringLiteral("for a"), {});
        store.add(QStringLiteral("b"), QStringLiteral("human"), QStringLiteral("for b"), {});

        const QJsonArray forA = store.list(QStringLiteral("a"), true);
        QCOMPARE(forA.size(), 1);
        QCOMPARE(forA[0].toObject()[QStringLiteral("comment")].toString(),
                 QStringLiteral("for a"));
        QCOMPARE(store.consume(QStringLiteral("b")).size(), 1);
        // Consuming b must not have touched a's note.
        QCOMPARE(store.list(QStringLiteral("a"), true).size(), 1);
    }
};

QTEST_MAIN(TestFeedbackStore)
#include "test_feedbackstore.moc"
