// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

import 'package:air/core/core.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/message_bubble/message_bubble.dart';
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/l10n/app_localizations.dart';
import 'package:air/share/share_payload.dart';
import 'package:air/share/share_screen.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../helpers.dart';

import 'package:air/share/share_cubit.dart';

class MockShareCubit extends MockCubit<ShareState> implements IOSShareCubit {}

final userProfiles = [
  UiUserProfile(userId: 1.userId(), displayName: 'Alice'),
  UiUserProfile(userId: 2.userId(), displayName: 'Bob'),
];

final chats = [
  UiChatDetails(
    id: 1.chatId(),
    status: const UiChatStatus.active(),
    isApq: false,
    chatType: UiChatType_Connection(userProfiles[0]),
    unreadMessages: 0,
    lastUsed: DateTime.parse('2023-01-01T00:00:00.000Z'),
    lastMessage: null,
    mutedUntil: null,
    pendingCommitFailed: false,
  ),
  UiChatDetails(
    id: 2.chatId(),
    status: const UiChatStatus.active(),
    isApq: false,
    chatType: UiChatType_Connection(userProfiles[1]),
    unreadMessages: 0,
    lastUsed: DateTime.parse('2023-01-02T00:00:00.000Z'),
    lastMessage: null,
    mutedUntil: null,
    pendingCommitFailed: false,
  ),
  UiChatDetails(
    id: 3.chatId(),
    status: const UiChatStatus.active(),
    isApq: false,
    chatType: const UiChatType_Group(
      UiChatAttributes(title: 'Group', picture: null),
    ),
    unreadMessages: 0,
    lastUsed: DateTime.parse('2023-01-03T00:00:00.000Z'),
    lastMessage: null,
    mutedUntil: null,
    pendingCommitFailed: false,
  ),
];

final nextButtonFinder = find.byKey(const Key('shareNextButton'));
final sendButtonFinder = find.byKey(const Key('shareSendButton'));

/// The header's leading action, the trailing one being the only one keyed.
/// Only the step on top of the stack is on screen, so this resolves to the
/// page a test is looking at.
final headerActionFinder = find.descendant(
  of: find.byType(DialogHeader),
  matching: find.byWidgetPredicate(
    (widget) => widget is DialogHeaderAction && widget.key == null,
  ),
);

/// An action without a handler is the disabled one.
bool isEnabled(WidgetTester tester, Finder finder) =>
    tester.widget<DialogHeaderAction>(finder).onPressed != null;

AppIconType headerGlyph(WidgetTester tester) =>
    tester.widget<DialogHeaderAction>(headerActionFinder).icon;

final loadedState = ShareState(
  loaded: true,
  signedIn: true,
  chats: chats,
  sendStatus: const UiShareSendStatus.idle(),
);

const signedOutState = ShareState(
  loaded: true,
  signedIn: false,
  chats: [],
  sendStatus: UiShareSendStatus.idle(),
);

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  setUpAll(() {
    registerFallbackValue(<ChatId>[]);
  });

  late MockShareCubit shareCubit;

  setUp(() {
    shareCubit = MockShareCubit();
  });

  // The share host never mounts the screen with nothing to share, so the
  // default payload carries content.
  Widget buildSubject({
    SharePayload payload = const SharePayload(text: 'https://air.ms'),
  }) => MaterialApp(
    debugShowCheckedModeBanner: false,
    theme: testLightTheme,
    localizationsDelegates: AppLocalizations.localizationsDelegates,
    home: BlocProvider<IOSShareCubit>.value(
      value: shareCubit,
      child: ShareScreenView(payload: payload),
    ),
  );

  /// Picks [chatNames] and moves on to the compose step.
  Future<void> pickAndContinue(
    WidgetTester tester, {
    List<String> chatNames = const ['Alice'],
  }) async {
    for (final name in chatNames) {
      await tester.tap(find.text(name));
    }
    await tester.pumpAndSettle();
    await tester.tap(nextButtonFinder);
    await tester.pumpAndSettle();
  }

  group('ShareScreenView', () {
    testWidgets('shows a spinner while loading', (tester) async {
      sizeView(tester, phoneViewSize);
      when(() => shareCubit.state).thenReturn(
        const ShareState(
          loaded: false,
          signedIn: false,
          chats: [],
          sendStatus: UiShareSendStatus.idle(),
        ),
      );

      await tester.pumpWidget(buildSubject());

      expect(find.byType(CircularProgressIndicator), findsOneWidget);
    });

    testWidgets('shows the signed-out state', (tester) async {
      sizeView(tester, phoneViewSize);
      when(() => shareCubit.state).thenReturn(signedOutState);

      // The button that leaves for the main app only exists on Android. The
      // test framework asserts the override is unset again by the time the
      // body returns.
      debugDefaultTargetPlatformOverride = TargetPlatform.android;
      try {
        await tester.pumpWidget(buildSubject());
        await tester.pumpAndSettle();

        expect(
          find.text('Sign in to Air first to share content.'),
          findsOneWidget,
        );
        expect(find.text('Open Air'), findsOneWidget);
        // There is nothing to pick a chat for, so neither step is reachable.
        expect(nextButtonFinder, findsNothing);
        expect(sendButtonFinder, findsNothing);

        await expectLater(
          find.byType(MaterialApp),
          matchesGoldenFile('goldens/share_screen_signed_out.png'),
        );
      } finally {
        debugDefaultTargetPlatformOverride = null;
      }
    });

    testWidgets('offers no way into the app off Android', (tester) async {
      sizeView(tester, phoneViewSize);
      when(() => shareCubit.state).thenReturn(signedOutState);

      debugDefaultTargetPlatformOverride = TargetPlatform.iOS;
      try {
        await tester.pumpWidget(buildSubject());
        await tester.pumpAndSettle();

        expect(
          find.text('Sign in to Air first to share content.'),
          findsOneWidget,
        );
        expect(find.text('Open Air'), findsNothing);
      } finally {
        debugDefaultTargetPlatformOverride = null;
      }
    });

    testWidgets('shows the chat picker and selects chats', (tester) async {
      sizeView(tester, phoneViewSize);
      when(() => shareCubit.state).thenReturn(loadedState);

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      expect(find.text('Alice'), findsOneWidget);
      expect(find.text('Bob'), findsOneWidget);
      expect(find.text('Group'), findsOneWidget);
      // The picker step keeps the whole sheet for the list, so what is being
      // shared is only previewed on the compose step.
      expect(find.text('https://air.ms'), findsNothing);
      // The OS presents the sheet, so there is no screen behind it to go
      // back to.
      expect(headerGlyph(tester), AppIconType.x);

      // Two adjacent rows, so the golden also covers the divider between
      // two selected rows.
      await tester.tap(find.text('Bob'));
      await tester.tap(find.text('Group'));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/share_screen_picker.png'),
      );
    });

    // The bug the two-step layout fixes: a preview and a composer sharing the
    // sheet left the list a strip two rows tall.
    testWidgets('gives the picker every row the sheet has', (tester) async {
      sizeView(tester, phoneViewSize);
      when(() => shareCubit.state).thenReturn(loadedState);

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      expectFillsModal(tester, find.byType(ListView), phoneViewSize);
    });

    testWidgets('filters chats by search query', (tester) async {
      sizeView(tester, phoneViewSize);
      when(() => shareCubit.state).thenReturn(loadedState);

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      await tester.enterText(find.byType(TextField).first, 'gro');
      await tester.pumpAndSettle();

      expect(find.text('Group'), findsOneWidget);
      expect(find.text('Alice'), findsNothing);
      expect(find.text('Bob'), findsNothing);
    });

    testWidgets('holds the picker back until a chat is picked', (tester) async {
      sizeView(tester, phoneViewSize);
      when(() => shareCubit.state).thenReturn(loadedState);

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      expect(isEnabled(tester, nextButtonFinder), isFalse);

      await tester.tap(find.text('Alice'));
      await tester.pumpAndSettle();
      expect(isEnabled(tester, nextButtonFinder), isTrue);
    });

    testWidgets('deselects a chat that is tapped again', (tester) async {
      sizeView(tester, phoneViewSize);
      when(() => shareCubit.state).thenReturn(loadedState);

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      await tester.tap(find.text('Alice'));
      await tester.pumpAndSettle();
      expect(isEnabled(tester, nextButtonFinder), isTrue);

      await tester.tap(find.text('Alice'));
      await tester.pumpAndSettle();
      expect(isEnabled(tester, nextButtonFinder), isFalse);
    });

    testWidgets('names the chosen chats and previews the content', (
      tester,
    ) async {
      sizeView(tester, phoneViewSize);
      when(() => shareCubit.state).thenReturn(loadedState);

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();
      await pickAndContinue(tester, chatNames: ['Bob', 'Group']);

      expect(find.text('To: Bob and Group'), findsOneWidget);
      expect(find.text('https://air.ms'), findsOneWidget);
      // The picker sits below the compose step rather than beside it.
      expect(find.text('Alice'), findsNothing);

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/share_screen_compose.png'),
      );
    });

    testWidgets('previews a file in the bubble the chat gives it', (
      tester,
    ) async {
      sizeView(tester, phoneViewSize);
      when(() => shareCubit.state).thenReturn(loadedState);
      final directory = Directory.systemTemp.createTempSync('share-preview');
      addTearDown(() => directory.deleteSync(recursive: true));
      final file = File('${directory.path}/report.pdf')
        ..writeAsBytesSync(List.filled(2048, 0));

      await tester.pumpWidget(
        buildSubject(
          payload: SharePayload(
            attachments: [
              UiSharedAttachment(path: file.path, mimeType: 'application/pdf'),
            ],
          ),
        ),
      );
      await tester.pumpAndSettle();
      await pickAndContinue(tester);

      expect(
        find.descendant(
          of: find.byType(MessageBubble),
          matching: find.text('report.pdf'),
        ),
        findsOneWidget,
      );
      expect(find.text('2.05 KB'), findsOneWidget);
    });

    testWidgets('returns to the picker with the selection kept', (
      tester,
    ) async {
      sizeView(tester, phoneViewSize);
      when(() => shareCubit.state).thenReturn(loadedState);

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();
      await pickAndContinue(tester);

      expect(headerGlyph(tester), AppIconType.arrowLeft);
      await tester.tap(headerActionFinder);
      await tester.pumpAndSettle();

      expect(find.text('Alice'), findsOneWidget);
      expect(isEnabled(tester, nextButtonFinder), isTrue);
    });

    testWidgets('sends the payload to the selected chat', (tester) async {
      sizeView(tester, phoneViewSize);
      when(() => shareCubit.state).thenReturn(loadedState);
      when(() => shareCubit.resetSendStatus()).thenReturn(null);
      when(
        () => shareCubit.send(
          chatIds: any(named: 'chatIds'),
          attachments: any(named: 'attachments'),
          message: any(named: 'message'),
        ),
      ).thenAnswer((_) async {});

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();
      await pickAndContinue(tester);

      await tester.enterText(find.byType(TextField), 'look at this');
      await tester.tap(sendButtonFinder);
      await tester.pumpAndSettle();

      verify(
        () => shareCubit.send(
          chatIds: [1.chatId()],
          attachments: [],
          message: 'https://air.ms\n\nlook at this',
        ),
      ).called(1);
    });

    testWidgets('sends the payload to several chats', (tester) async {
      sizeView(tester, phoneViewSize);
      when(() => shareCubit.state).thenReturn(loadedState);
      when(() => shareCubit.resetSendStatus()).thenReturn(null);
      when(
        () => shareCubit.send(
          chatIds: any(named: 'chatIds'),
          attachments: any(named: 'attachments'),
          message: any(named: 'message'),
        ),
      ).thenAnswer((_) async {});

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();
      await pickAndContinue(tester, chatNames: ['Bob', 'Group']);

      await tester.tap(sendButtonFinder);
      await tester.pumpAndSettle();

      verify(
        () => shareCubit.send(
          chatIds: [2.chatId(), 3.chatId()],
          attachments: [],
          message: 'https://air.ms',
        ),
      ).called(1);
    });

    testWidgets('opens on the compose step for a direct share target', (
      tester,
    ) async {
      sizeView(tester, phoneViewSize);
      when(() => shareCubit.state).thenReturn(loadedState);
      whenListen(
        shareCubit,
        Stream.fromIterable([loadedState]),
        initialState: const ShareState(
          loaded: false,
          signedIn: false,
          chats: [],
          sendStatus: UiShareSendStatus.idle(),
        ),
      );
      when(() => shareCubit.chatIdForShareTarget(any())).thenReturn(2.chatId());

      await tester.pumpWidget(
        buildSubject(
          payload: SharePayload(
            text: 'hi',
            shareTargetIdentifier: 2.chatId().uuid.toString(),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('To: Bob'), findsOneWidget);
      expect(sendButtonFinder, findsOneWidget);

      // Back still leads to the picker, with the donated chat selected.
      await tester.tap(headerActionFinder);
      await tester.pumpAndSettle();

      expect(find.text('Alice'), findsOneWidget);
      expect(isEnabled(tester, nextButtonFinder), isTrue);
    });

    testWidgets('shows the upload progress', (tester) async {
      sizeView(tester, phoneViewSize);
      when(() => shareCubit.state).thenReturn(
        loadedState.copyWith(
          sendStatus: const UiShareSendStatus.uploading(
            current: 1,
            total: 3,
            progress: 0.25,
          ),
        ),
      );

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();
      await pickAndContinue(tester);

      final indicator = tester.widget<CircularProgressIndicator>(
        find.byType(CircularProgressIndicator),
      );
      expect(indicator.value, 0.25);
      expect(find.text('Uploading 1 of 3…'), findsOneWidget);
      // A send under way cannot be taken back to the picker or repeated, so
      // the only way out of the sheet is closing it.
      expect(headerGlyph(tester), AppIconType.x);
      expect(isEnabled(tester, sendButtonFinder), isFalse);
    });

    testWidgets('shows the queued state', (tester) async {
      sizeView(tester, phoneViewSize);
      when(() => shareCubit.state).thenReturn(
        loadedState.copyWith(sendStatus: const UiShareSendStatus.queued()),
      );

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();
      await pickAndContinue(tester);

      expect(
        find.textContaining('will be sent when you next open Air'),
        findsOneWidget,
      );
    });

    testWidgets('reports items the host could not hand over', (tester) async {
      sizeView(tester, phoneViewSize);
      when(() => shareCubit.state).thenReturn(loadedState);

      await tester.pumpWidget(
        buildSubject(
          payload: const SharePayload(
            text: 'https://air.ms',
            droppedAttachments: 2,
          ),
        ),
      );
      await tester.pumpAndSettle();
      await pickAndContinue(tester);

      expect(find.text("2 items couldn't be shared."), findsOneWidget);
      // The rest of the content can still be sent.
      expect(isEnabled(tester, sendButtonFinder), isTrue);
    });

    testWidgets('does not close on success when items were dropped', (
      tester,
    ) async {
      sizeView(tester, phoneViewSize);
      when(() => shareCubit.state).thenReturn(
        loadedState.copyWith(sendStatus: const UiShareSendStatus.done()),
      );

      await tester.pumpWidget(
        buildSubject(
          payload: const SharePayload(
            text: 'https://air.ms',
            droppedAttachments: 1,
          ),
        ),
      );
      await tester.pumpAndSettle();
      await pickAndContinue(tester);

      expect(find.text("1 item couldn't be shared."), findsOneWidget);
      expect(find.widgetWithText(OutlinedButton, 'Done'), findsOneWidget);
      expect(find.byType(CircularProgressIndicator), findsNothing);
    });

    testWidgets('shows an error view when nothing was handed over', (
      tester,
    ) async {
      sizeView(tester, phoneViewSize);
      when(() => shareCubit.state).thenReturn(loadedState);

      await tester.pumpWidget(
        buildSubject(payload: const SharePayload(droppedAttachments: 1)),
      );
      await tester.pumpAndSettle();

      expect(
        find.text("This content couldn't be shared to Air."),
        findsOneWidget,
      );
      expect(find.widgetWithText(OutlinedButton, 'Close'), findsOneWidget);
      // Nothing to send, so there is no picker and no way on.
      expect(find.text('Alice'), findsNothing);
      expect(nextButtonFinder, findsNothing);
    });

    testWidgets('shows the failed state', (tester) async {
      sizeView(tester, phoneViewSize);
      when(() => shareCubit.state).thenReturn(
        loadedState.copyWith(
          sendStatus: const UiShareSendStatus.failed(
            error: UiShareSendError.other(),
          ),
        ),
      );

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();
      await pickAndContinue(tester);

      expect(find.text('Failed to send. Try again.'), findsOneWidget);
      // A failure can be retried.
      expect(isEnabled(tester, sendButtonFinder), isTrue);
    });
  });
}
