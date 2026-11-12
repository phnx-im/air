// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:typed_data';

import 'package:air/app.dart' show scaffoldMessengerKey;
import 'package:air/core/core.dart';
import 'package:air/core/lib.dart' show U8Array32;
import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/message_input/message_input_link_preview.dart';
import 'package:air/ds/patterns/message_input/message_input_quote.dart';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/chat/chat_screen.dart';
import 'package:air/features/chat/share_target_publisher.dart';
import 'package:air/features/chat/chats_repository.dart';
import 'package:air/features/message_list/message_list_cubit.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../../helpers.dart';
import '../../mocks.dart';
import '../chat_list/chat_list_content_test.dart';
import 'message_list_test.dart';

final _chat = chats[2]; // Group chat, isConfirmed = true

final _members = [1.userId(), 2.userId(), 3.userId()];

final _navState = NavigationState.home(
  home: HomeNavigationState(chatId: _chat.id),
);

/// The composer's draft debounce.
const _draftDebounce = Duration(seconds: 1);

/// The composer's link preview debounce.
const _linkPreviewDebounce = Duration(milliseconds: 300);

const _linkUrl = 'https://air.ms/blog/link-previews';

const _linkCard = UiLinkPreviewCard(
  url: _linkUrl,
  title: 'Link previews in Air',
  description: 'How a card is resolved by the sender and travels inside.',
  kind: UiLinkPreviewKind.default_,
);

UiChatDetails _chatWithDraft(UiMessageDraft draft) => UiChatDetails(
  id: _chat.id,
  status: _chat.status,
  chatType: _chat.chatType,
  lastUsed: _chat.lastUsed,
  unreadMessages: _chat.unreadMessages,
  lastMessage: _chat.lastMessage,
  draft: draft,
  isApq: _chat.isApq,
  isSelfChat: _chat.isSelfChat,
  mutedUntil: _chat.mutedUntil,
  pendingCommitFailed: false,
  resyncFailed: false,
);

final _replyDraft = UiMessageDraft(
  message: 'Read this',
  previewText: 'Read this',
  inReplyTo: (
    UiMimiId(field0: U8Array32(Uint8List(32))),
    UiInReplyToMessage.resolved(
      messageId: 1.messageId(),
      sender: 2.userId(),
      mimiContent: textContent('Got a link for me?'),
    ),
  ),
  updatedAt: DateTime.parse('2023-01-01T00:00:00.000Z'),
  isCommitted: true,
);

final _editDraft = UiMessageDraft(
  message: 'Corrected text',
  previewText: 'Corrected text',
  editingId: 1.messageId(),
  updatedAt: DateTime.parse('2023-01-01T00:00:00.000Z'),
  isCommitted: true,
);

void main() {
  late AppLocalizations loc;

  late MockNavigationCubit navigationCubit;
  late MockUserCubit userCubit;
  late MockUsersCubit usersCubit;
  late MockChatDetailsCubit chatDetailsCubit;
  late MockMessageListCubit messageListCubit;
  late MockUserSettingsCubit userSettingsCubit;
  late TextEditingController inputController;

  setUpAll(() async {
    registerFallbackValue(0.messageId());
    registerFallbackValue(0.userId());
    loc = await AppLocalizations.delegate.load(const Locale('en'));
  });

  setUp(() {
    navigationCubit = MockNavigationCubit();
    userCubit = MockUserCubit();
    usersCubit = MockUsersCubit();
    chatDetailsCubit = MockChatDetailsCubit();
    messageListCubit = MockMessageListCubit();
    userSettingsCubit = MockUserSettingsCubit();
    // The composer owns the controller it is handed and disposes it, so the
    // test must not dispose it a second time.
    inputController = TextEditingController();

    when(() => userCubit.state).thenReturn(MockUiUser(id: 1));
    when(() => usersCubit.state)
        .thenReturn(MockUsersState(profiles: userProfiles));
    when(() => chatDetailsCubit.state)
        .thenReturn(ChatDetailsState(chat: _chat, members: _members));
    when(
      () => chatDetailsCubit.markAsRead(
        untilMessageId: any(named: "untilMessageId"),
        untilTimestamp: any(named: "untilTimestamp"),
      ),
    ).thenAnswer((_) async {});
    when(
      () => chatDetailsCubit.storeDraft(
        draftMessage: any(named: "draftMessage"),
        isCommitted: any(named: "isCommitted"),
      ),
    ).thenAnswer((_) async {});
    when(() => userSettingsCubit.state).thenReturn(const UserSettings());
    when(() => navigationCubit.state).thenReturn(_navState);

    messageListCubit.setState(messages);
  });

  // The snackbar goes through the global scaffold messenger, so the app under
  // test has to carry the very key `showSnackBarStandalone` reaches for.
  Widget buildSubject() => MultiBlocProvider(
    providers: [
      RepositoryProvider<ChatsRepository>.value(
        value: FakeChatsRepository(chats),
      ),
      BlocProvider<NavigationCubit>.value(value: navigationCubit),
      BlocProvider<UserCubit>.value(value: userCubit),
      BlocProvider<UsersCubit>.value(value: usersCubit),
      BlocProvider<ChatDetailsCubit>.value(value: chatDetailsCubit),
      BlocProvider<MessageListCubit>.value(value: messageListCubit),
      BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
      RepositoryProvider<ShareTargetPublisher>.value(
        value: MockShareTargetPublisher(),
      ),
    ],
    child: Builder(
      builder: (context) => MaterialApp(
        debugShowCheckedModeBanner: false,
        scaffoldMessengerKey: scaffoldMessengerKey,
        theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        home: Scaffold(
          body: ChatScreenView(
            createMessageCubit: createMockMessageCubit,
            textEditingController: inputController,
          ),
        ),
      ),
    ),
  );

  group('MessageComposer send failure', () {
    final sendButton = find.byWidgetPredicate(
      (widget) => widget is ButtonIcon && widget.icon == AppIconType.arrowUp,
    );

    Finder errorSnackbar() => find.text(loc.composer_error_sendMessage);

    Future<void> typeAndSend(WidgetTester tester, String text) async {
      await tester.enterText(find.byType(TextField), text);
      await tester.pumpAndSettle();
      await tester.tap(sendButton);
      await tester.pumpAndSettle();
    }

    testWidgets('restores the text and shows an error snackbar', (
      tester,
    ) async {
      when(() => chatDetailsCubit.sendMessage(any()))
          .thenAnswer((_) async => throw Exception('send failed'));

      await tester.pumpWidget(buildSubject());
      await tester.pump();

      await typeAndSend(tester, 'Hello there');

      expect(errorSnackbar(), findsOneWidget);
      expect(inputController.text, 'Hello there');
    });

    for (final (error, label) in [
      (
        const UiSendMessageError.editWindowPassed(),
        (AppLocalizations loc) => loc.composer_error_editWindowPassed,
      ),
      (
        const UiSendMessageError.editLimitReached(),
        (AppLocalizations loc) => loc.composer_error_editLimitReached,
      ),
    ]) {
      // Only an edit is refused with these errors. The text stays for sending
      // it as a new message.
      testWidgets('explains a refused edit: $error', (tester) async {
        when(() => chatDetailsCubit.sendMessage(any()))
            .thenAnswer((_) async => throw error);

        await tester.pumpWidget(buildSubject());
        await tester.pump();

        await typeAndSend(tester, 'Corrected text');

        expect(find.text(label(loc)), findsOneWidget);
        expect(inputController.text, 'Corrected text');
      });
    }

    testWidgets('keeps text typed while the send was in flight', (
      tester,
    ) async {
      final sent = Completer<void>();
      when(() => chatDetailsCubit.sendMessage(any()))
          .thenAnswer((_) => sent.future);

      await tester.pumpWidget(buildSubject());
      await tester.pump();

      await typeAndSend(tester, 'Hello there');
      expect(inputController.text, isEmpty);

      await tester.enterText(find.byType(TextField), 'Second thoughts');
      await tester.pumpAndSettle();

      sent.completeError(Exception('send failed'));
      await tester.pumpAndSettle();

      expect(errorSnackbar(), findsOneWidget);
      expect(inputController.text, 'Second thoughts');
    });

    testWidgets('leaves the input empty when the send succeeds', (
      tester,
    ) async {
      when(() => chatDetailsCubit.sendMessage(any())).thenAnswer((_) async {});

      await tester.pumpWidget(buildSubject());
      await tester.pump();

      await typeAndSend(tester, 'Hello there');

      expect(errorSnackbar(), findsNothing);
      expect(inputController.text, isEmpty);
      verify(() => chatDetailsCubit.sendMessage('Hello there')).called(1);
    });

    testWidgets('does not store the emptied input as a draft', (tester) async {
      when(() => chatDetailsCubit.sendMessage(any())).thenAnswer((_) async {});

      await tester.pumpWidget(buildSubject());
      await tester.pump();

      await typeAndSend(tester, 'Hello there');
      // Past the debounce the cleared input would have been stored, which is
      // the write that races the send.
      await tester.pump(_draftDebounce);

      verifyNever(
        () => chatDetailsCubit.storeDraft(draftMessage: '', isCommitted: false),
      );
    });
  });

  group('MessageComposer disposal', () {
    testWidgets('drops the pending draft store instead of un-committing', (
      tester,
    ) async {
      await tester.pumpWidget(buildSubject());
      await tester.pump();

      await tester.enterText(find.byType(TextField), 'Hello there');
      await tester.pumpAndSettle();

      // Tear the tree down while the keystroke's store is still debounced.
      await tester.pumpWidget(const SizedBox());
      await tester.pump(_draftDebounce);

      verify(
        () => chatDetailsCubit.storeDraft(
          draftMessage: 'Hello there',
          isCommitted: true,
        ),
      ).called(1);
      verifyNever(
        () => chatDetailsCubit.storeDraft(
          draftMessage: any(named: "draftMessage"),
          isCommitted: false,
        ),
      );
    });
  });

  group('MessageComposer link preview', () {
    const experimental = UserSettings(
      developerMode: true,
      experimentalFeatures: true,
    );

    Finder staged() => find.byType(MessageInputLinkPreview);

    void stage(UiComposerLinkPreview preview, {UiMessageDraft? draft}) {
      when(() => chatDetailsCubit.state).thenReturn(
        ChatDetailsState(
          chat: draft == null ? _chat : _chatWithDraft(draft),
          members: _members,
          linkPreview: preview,
        ),
      );
    }

    setUp(() {
      when(() => chatDetailsCubit.updateLinkPreviews(any()))
          .thenAnswer((_) async {});
      when(() => chatDetailsCubit.dismissLinkPreview(any()))
          .thenAnswer((_) async {});
      when(() => chatDetailsCubit.composerLinkPreviewImage(any()))
          .thenAnswer((_) async => null);
    });

    testWidgets('shows the url while loading', (tester) async {
      stage(const UiComposerLinkPreview.loading(url: _linkUrl));

      await tester.pumpWidget(buildSubject());
      await tester.pump();

      expect(staged(), findsOneWidget);
      expect(
        find.descendant(of: staged(), matching: find.text(_linkUrl)),
        findsOneWidget,
      );
    });

    testWidgets('shows the card once ready', (tester) async {
      stage(
        const UiComposerLinkPreview.ready(
          card: _linkCard,
          image: UiImageMetadata(width: 200, height: 200),
        ),
      );

      await tester.pumpWidget(buildSubject());
      await tester.pump();

      expect(find.text(_linkCard.title), findsOneWidget);
      expect(find.text(_linkCard.description), findsOneWidget);
      verify(() => chatDetailsCubit.composerLinkPreviewImage(_linkUrl))
          .called(1);
    });

    testWidgets('sits above the reply quote', (tester) async {
      stage(
        const UiComposerLinkPreview.ready(card: _linkCard),
        draft: _replyDraft,
      );

      await tester.pumpWidget(buildSubject());
      await tester.pump();

      expect(
        tester.getRect(staged()).bottom,
        lessThanOrEqualTo(tester.getRect(find.byType(MessageInputQuote)).top),
      );
    });

    testWidgets('is hidden while editing', (tester) async {
      stage(
        const UiComposerLinkPreview.ready(card: _linkCard),
        draft: _editDraft,
      );

      await tester.pumpWidget(buildSubject());
      await tester.pump();

      expect(staged(), findsNothing);
      expect(find.text(_linkCard.title), findsNothing);
    });

    testWidgets('dismissing the card reaches the cubit', (tester) async {
      stage(const UiComposerLinkPreview.ready(card: _linkCard));

      await tester.pumpWidget(buildSubject());
      await tester.pump();
      await tester.tap(
        find.descendant(
          of: staged(),
          matching: find.byWidgetPredicate(
            (w) => w is ButtonIcon && w.icon == AppIconType.x,
          ),
        ),
      );

      verify(() => chatDetailsCubit.dismissLinkPreview(_linkUrl)).called(1);
    });

    testWidgets('resolves the link after the debounce', (tester) async {
      when(() => userSettingsCubit.state).thenReturn(experimental);

      await tester.pumpWidget(buildSubject());
      await tester.pump();

      await tester.enterText(find.byType(TextField), 'See $_linkUrl');
      await tester.pump(_linkPreviewDebounce - const Duration(milliseconds: 1));
      verifyNever(() => chatDetailsCubit.updateLinkPreviews(any()));

      await tester.pump(const Duration(milliseconds: 1));
      verify(() => chatDetailsCubit.updateLinkPreviews('See $_linkUrl'))
          .called(1);
    });

    testWidgets('resolves only the last text typed', (tester) async {
      when(() => userSettingsCubit.state).thenReturn(experimental);

      await tester.pumpWidget(buildSubject());
      await tester.pump();

      await tester.enterText(find.byType(TextField), 'See https://air');
      await tester.pump(const Duration(milliseconds: 100));
      await tester.enterText(find.byType(TextField), 'See $_linkUrl');
      await tester.pump(_linkPreviewDebounce);

      expect(
        verify(() => chatDetailsCubit.updateLinkPreviews(captureAny()))
            .captured,
        ['See $_linkUrl'],
      );
    });

    testWidgets('leaves links alone without experimental features', (
      tester,
    ) async {
      await tester.pumpWidget(buildSubject());
      await tester.pump();

      await tester.enterText(find.byType(TextField), 'See $_linkUrl');
      await tester.pump(_linkPreviewDebounce);

      verifyNever(() => chatDetailsCubit.updateLinkPreviews(any()));
    });

    testWidgets('leaves links alone while editing', (tester) async {
      when(() => userSettingsCubit.state).thenReturn(experimental);
      when(() => chatDetailsCubit.state).thenReturn(
        ChatDetailsState(chat: _chatWithDraft(_editDraft), members: _members),
      );

      await tester.pumpWidget(buildSubject());
      await tester.pump();

      await tester.enterText(find.byType(TextField), 'See $_linkUrl');
      await tester.pump(_linkPreviewDebounce);

      verifyNever(() => chatDetailsCubit.updateLinkPreviews(any()));
    });

    testWidgets('renders correctly above a reply', (tester) async {
      stage(
        const UiComposerLinkPreview.ready(
          card: _linkCard,
          image: UiImageMetadata(
            blurhash: 'LEHLk~WB2yk8pyo0adR*.7kCMdnj',
            width: 200,
            height: 200,
          ),
        ),
        draft: _replyDraft,
      );

      await tester.pumpWidget(buildSubject());
      await tester.pump();
      await tester.enterText(find.byType(TextField), 'Read this');
      await tester.pump(_draftDebounce);

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/composer_link_preview.png'),
      );
    });
  });
}
