// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/chat/chat_screen.dart';
import 'package:air/features/chat/chats_repository.dart' as chats_repository;
import 'package:air/features/chat_list/chat_list_view.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/app_localizations.dart';
import 'package:air/features/message_list/message_list_cubit.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/navigation/app_tab_bar.dart';
import 'package:air/features/home/home_screen.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:device_frame/device_frame.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:provider/provider.dart';
import 'package:provider/single_child_widget.dart';
import 'package:system_date_time_format/system_date_time_format.dart';

import '../helpers.dart';
import '../features/message_list/message_list_test.dart';
import '../mocks.dart';
import 'content.dart';
import 'product_shot.dart';
import 'product_shot_device.dart';

/// Override with `--dart-define=GOLDENS_DIR=path/to/dir`.
const _goldensDir = String.fromEnvironment(
  'GOLDENS_DIR',
  defaultValue: 'goldens',
);

/// Override with `--dart-define=FORCE_RENDER_ALL_PLATFORMS=true`
const _renderAllPlatforms = bool.fromEnvironment(
  'FORCE_RENDER_ALL_PLATFORMS',
  defaultValue: false,
);

String _golden(String name) => '$_goldensDir/$name';

const androidPhysicalSize = Size(2160, 3840);
const iosPhysicalSize = Size(1290, 2796);
// One of the sizes accepted by the Mac App Store (16:10 retina).
const laptopPhysicalSize = Size(2880, 1800);

/// The marketing canvas size for a framed product shot, distinct from the
/// device's own screen resolution (used as-is for the frameless shots).
Size _canvasSizeFor(TargetPlatform platform) {
  switch (platform) {
    case TargetPlatform.android:
      return androidPhysicalSize;
    case TargetPlatform.iOS:
      return iosPhysicalSize;
    case TargetPlatform.macOS:
    case TargetPlatform.windows:
    case TargetPlatform.linux:
      // Laptop/desktop devices are landscape, so they share the macOS
      // canvas rather than being squeezed into a portrait phone canvas.
      return laptopPhysicalSize;
    default:
      throw "Unsupported platform";
  }
}

bool _isDesktopPlatform(TargetPlatform platform) => switch (platform) {
  TargetPlatform.macOS ||
  TargetPlatform.windows ||
  TargetPlatform.linux => true,
  _ => false,
};

/// On desktop the real app always shows the chat list beside the open chat,
/// so the shot needs the split layout instead of just the chat on its own.
Widget _chatShot(TargetPlatform platform, Widget chat) =>
    _isDesktopPlatform(platform)
    ? HomeScreenDesktopLayout(chatList: const ChatListView(), chat: chat)
    : chat;

class ProductShotInfo {
  const ProductShotInfo({
    required this.hostPlatform,
    required this.targetPlatform,
  });

  final String hostPlatform;
  final TargetPlatform targetPlatform;
}

/// Providers + app scaffolding shared by every product shot subject.
///
/// [shot] is a [ProductShot] for the marketing variant or a [DeviceFrame] for
/// the [frameless] one, which also centers it on a transparent background
/// instead of stretching it under the marketing canvas.
Widget buildProductShotSubject({
  required List<SingleChildWidget> providers,
  required Widget shot,
  bool frameless = false,
}) => MultiProvider(
  providers: providers,
  child: SDTFScope(
    child: Builder(
      builder: (context) => MaterialApp(
        debugShowCheckedModeBanner: false,
        theme: testLightTheme,
        themeMode: .light,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        home: Material(
          color: frameless ? Colors.transparent : null,
          child: MediaQuery(
            data: MediaQuery.of(context).copyWith(platformBrightness: .light),
            child: frameless ? Center(child: shot) : shot,
          ),
        ),
      ),
    ),
  ),
);

void main() {
  setUpAll(() {
    registerFallbackValue(0.messageId());
    registerFallbackValue(0.userId());
    registerFallbackValue(0.attachmentId());
  });

  const productShotsMatrix = [
    ProductShotInfo(hostPlatform: 'macos', targetPlatform: TargetPlatform.iOS),
    ProductShotInfo(
      hostPlatform: 'macos',
      targetPlatform: TargetPlatform.macOS,
    ),
    ProductShotInfo(
      hostPlatform: 'linux',
      targetPlatform: TargetPlatform.android,
    ),
    ProductShotInfo(
      hostPlatform: 'linux',
      targetPlatform: TargetPlatform.linux,
    ),
    ProductShotInfo(
      hostPlatform: 'windows',
      targetPlatform: TargetPlatform.windows,
    ),
  ];

  group('Chat List Product Shots', () {
    final backgroundColor = Primitive.neutral(NeutralShade.s100);
    final titleColor = Primitive.neutral(NeutralShade.s800);
    final subtitleColor = Primitive.neutral(NeutralShade.s600);
    final frameColor = Primitive.neutral(NeutralShade.s300);
    const title = 'Secure messaging\nfor everyone.';
    const subtitle = 'Everything in Air is\nend-to-end encrypted.';

    late MockNavigationCubit navigationCubit;
    late MockUserCubit userCubit;
    late MockUsersCubit usersCubit;
    late MockUserSettingsCubit userSettingsCubit;

    setUp(() async {
      navigationCubit = MockNavigationCubit();
      userCubit = MockUserCubit();
      usersCubit = MockUsersCubit();
      userSettingsCubit = MockUserSettingsCubit();

      when(
        () => navigationCubit.state,
      ).thenReturn(const NavigationState.home());
      when(() => userCubit.state).thenReturn(MockUiUser(id: 10));
      when(() => usersCubit.state).thenReturn(
        MockUsersState(profiles: userProfiles, defaultUserId: ownId),
      );
      when(
        () => userSettingsCubit.state,
      ).thenReturn(const UserSettings(experimentalFeatures: false));
    });

    List<SingleChildWidget> buildProviders() => [
      RepositoryProvider<AttachmentsRepository>.value(
        value: MockAttachmentsRepository(),
      ),
      RepositoryProvider<chats_repository.ChatsRepository>.value(
        value: FakeChatsRepository(chats),
      ),
      BlocProvider<NavigationCubit>.value(value: navigationCubit),
      BlocProvider<UserCubit>.value(value: userCubit),
      BlocProvider<UsersCubit>.value(value: usersCubit),
      BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
    ];

    Widget chatListScreen() => const Stack(
      children: [
        Positioned.fill(child: ChatListView(scaffold: true)),
        Positioned(left: 0, right: 0, bottom: 0, child: AppTabBar()),
      ],
    );

    Widget buildSubject(ProductShotDevice device) => buildProductShotSubject(
      providers: buildProviders(),
      shot: ProductShot(
        size: _canvasSizeFor(device.platform),
        backgroundColor: backgroundColor,
        titleColor: titleColor,
        subtitleColor: subtitleColor,
        title: title,
        subtitle: subtitle,
        frameColor: frameColor,
        device: device,
        child: chatListScreen(),
      ),
    );

    Widget buildFramelessSubject(DeviceInfo device) => buildProductShotSubject(
      providers: buildProviders(),
      frameless: true,
      shot: DeviceFrame(device: device, screen: chatListScreen()),
    );

    for (final productShotInfo in productShotsMatrix) {
      final device = productShotInfo.targetPlatform.device;
      final deviceInfo = device.deviceInfo;
      final identifier = device.identifier;

      // Build the product shot with marketing
      testProductShot(
        "Chat List (${deviceInfo.name})",
        productShotInfo: productShotInfo,
        deviceInfo: deviceInfo,
        physicalSizeOverride: _canvasSizeFor(productShotInfo.targetPlatform),
        (tester) async {
          await tester.pumpWidget(buildSubject(device));
          await _precacheImages(tester);
          await tester.pumpAndSettle();

          await expectLater(
            find.byType(ProductShot),
            // Do not change the ios/android file names, as they are
            // referenced in stores/ios/en-US/screenshots and
            // stores/android/metadata/en-US/images/phone-screenshots
            matchesGoldenFile(_golden("chat_list.$identifier.png")),
          );
        },
      );

      // Build the product shot without marketing chrome
      testProductShot(
        "Chat List (${deviceInfo.name})",
        productShotInfo: productShotInfo,
        deviceInfo: deviceInfo,
        (tester) async {
          await tester.pumpWidget(buildFramelessSubject(deviceInfo));
          await _precacheImages(tester);
          await tester.pumpAndSettle();

          await expectLater(
            find.byType(DeviceFrame),
            matchesGoldenFile(_golden("chat_list.$identifier.png")),
          );
        },
      );
    }
  });

  group("Private Chat", () {
    final backgroundColor = Primitive.chromatic(Hue.orange, Shade.s50);
    final titleColor = Primitive.chromatic(Hue.orange, Shade.s800);
    final subtitleColor = Primitive.chromatic(Hue.orange, Shade.s600);
    final frameColor = Primitive.chromatic(Hue.orange, Shade.s300);
    const title = 'Connect with friends.';
    const subtitle = 'Send messages in private chats.';

    late MockNavigationCubit navigationCubit;
    late MockUserCubit userCubit;
    late MockUsersCubit contactsCubit;
    late MockChatDetailsCubit chatDetailsCubit;
    late MockMessageListCubit messageListCubit;
    late MockUserSettingsCubit userSettingsCubit;
    late MockAttachmentsRepository attachmentsRepository;

    setUp(() async {
      navigationCubit = MockNavigationCubit();
      userCubit = MockUserCubit();
      contactsCubit = MockUsersCubit();
      chatDetailsCubit = MockChatDetailsCubit();
      messageListCubit = MockMessageListCubit();
      userSettingsCubit = MockUserSettingsCubit();
      attachmentsRepository = MockAttachmentsRepository();

      final chat = privateChat;

      when(() => navigationCubit.state).thenReturn(
        NavigationState.home(home: HomeNavigationState(chatId: chat.id)),
      );
      when(() => userCubit.state).thenReturn(MockUiUser(id: ownIdx));
      when(
        () => contactsCubit.state,
      ).thenReturn(MockUsersState(profiles: userProfiles));
      when(
        () => chatDetailsCubit.state,
      ).thenReturn(ChatDetailsState(chat: chat, members: privateChatMembers));
      when(
        () => chatDetailsCubit.markAsRead(
          untilMessageId: any(named: "untilMessageId"),
          untilTimestamp: any(named: "untilTimestamp"),
        ),
      ).thenAnswer((_) => Future.value());
      when(
        () => chatDetailsCubit.storeDraft(
          draftMessage: any(named: "draftMessage"),
          isCommitted: any(named: "isCommitted"),
        ),
      ).thenAnswer((_) async => Future.value());
      when(() => userSettingsCubit.state).thenReturn(const UserSettings());
      messageListCubit.setState(privateChatMessages);
      _stubAttachments(attachmentsRepository, privateChatAttachmentImages);
      when(
        () => attachmentsRepository.statusStream(
          attachmentId: any(named: "attachmentId"),
        ),
      ).thenAnswer((_) => Stream.value(const UiAttachmentStatus.completed()));
    });

    List<SingleChildWidget> buildProviders() => [
      RepositoryProvider<AttachmentsRepository>.value(
        value: attachmentsRepository,
      ),
      RepositoryProvider<chats_repository.ChatsRepository>.value(
        value: FakeChatsRepository(chats),
      ),
      BlocProvider<NavigationCubit>.value(value: navigationCubit),
      BlocProvider<UserCubit>.value(value: userCubit),
      BlocProvider<UsersCubit>.value(value: contactsCubit),
      BlocProvider<ChatDetailsCubit>.value(value: chatDetailsCubit),
      BlocProvider<MessageListCubit>.value(value: messageListCubit),
      BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
    ];

    const chatScreen = ChatScreenView(
      createMessageCubit: createMockMessageCubit,
    );

    Widget buildSubject(ProductShotDevice device) => buildProductShotSubject(
      providers: buildProviders(),
      shot: ProductShot(
        size: _canvasSizeFor(device.platform),
        backgroundColor: backgroundColor,
        titleColor: titleColor,
        subtitleColor: subtitleColor,
        title: title,
        subtitle: subtitle,
        frameColor: frameColor,
        device: device,
        child: _chatShot(device.platform, chatScreen),
      ),
    );

    Widget buildFramelessSubject(DeviceInfo device, TargetPlatform platform) =>
        buildProductShotSubject(
          providers: buildProviders(),
          frameless: true,
          shot: DeviceFrame(
            device: device,
            screen: _chatShot(platform, chatScreen),
          ),
        );

    for (final productShotInfo in productShotsMatrix) {
      final device = productShotInfo.targetPlatform.device;
      final deviceInfo = device.deviceInfo;
      final identifier = device.identifier;

      // Build the product shot with marketing
      testProductShot(
        "Private Chat (${deviceInfo.name})",
        productShotInfo: productShotInfo,
        deviceInfo: deviceInfo,
        physicalSizeOverride: _canvasSizeFor(productShotInfo.targetPlatform),
        (tester) async {
          await tester.pumpWidget(buildSubject(device));
          await _precacheImages(tester);
          await tester.pumpAndSettle();

          await expectLater(
            find.byType(ProductShot),
            // Do not change the ios/android file names, as they are
            // referenced in stores/ios/en-US/screenshots and
            // stores/android/metadata/en-US/images/phone-screenshots
            matchesGoldenFile(_golden("private_chat.$identifier.png")),
          );
        },
      );

      // Build the product shot without marketing chrome
      testProductShot(
        "Private Chat (${productShotInfo.targetPlatform}, ${deviceInfo.name})",
        productShotInfo: productShotInfo,
        deviceInfo: deviceInfo,
        (tester) async {
          await tester.pumpWidget(
            buildFramelessSubject(deviceInfo, productShotInfo.targetPlatform),
          );
          await _precacheImages(tester);
          await tester.pumpAndSettle();

          await expectLater(
            find.byType(DeviceFrame),
            matchesGoldenFile(_golden("private_chat.$identifier.png")),
          );
        },
      );
    }
  });

  group("Group Chat", () {
    final backgroundColor = Primitive.chromatic(Hue.blue, Shade.s50);
    final titleColor = Primitive.chromatic(Hue.blue, Shade.s800);
    final subtitleColor = Primitive.chromatic(Hue.blue, Shade.s600);
    final frameColor = Primitive.chromatic(Hue.blue, Shade.s300);
    const title = 'Create group chats.';
    const subtitle = 'Message with multiple people.';

    late MockNavigationCubit navigationCubit;
    late MockUserCubit userCubit;
    late MockUsersCubit contactsCubit;
    late MockChatDetailsCubit chatDetailsCubit;
    late MockMessageListCubit messageListCubit;
    late MockUserSettingsCubit userSettingsCubit;
    late MockAttachmentsRepository attachmentsRepository;

    setUp(() async {
      navigationCubit = MockNavigationCubit();
      userCubit = MockUserCubit();
      contactsCubit = MockUsersCubit();
      chatDetailsCubit = MockChatDetailsCubit();
      messageListCubit = MockMessageListCubit();
      userSettingsCubit = MockUserSettingsCubit();
      attachmentsRepository = MockAttachmentsRepository();

      final chat = groupChat;

      when(() => navigationCubit.state).thenReturn(
        NavigationState.home(home: HomeNavigationState(chatId: chat.id)),
      );
      when(() => userCubit.state).thenReturn(MockUiUser(id: ownIdx));
      when(
        () => contactsCubit.state,
      ).thenReturn(MockUsersState(profiles: userProfiles));
      when(
        () => chatDetailsCubit.state,
      ).thenReturn(ChatDetailsState(chat: chat, members: groupChatMembers));
      when(
        () => chatDetailsCubit.markAsRead(
          untilMessageId: any(named: "untilMessageId"),
          untilTimestamp: any(named: "untilTimestamp"),
        ),
      ).thenAnswer((_) => Future.value());
      when(
        () => chatDetailsCubit.storeDraft(
          draftMessage: any(named: "draftMessage"),
          isCommitted: any(named: "isCommitted"),
        ),
      ).thenAnswer((_) async => Future.value());
      when(() => userSettingsCubit.state).thenReturn(const UserSettings());
      messageListCubit.setState(groupChatMessages);
      _stubAttachments(attachmentsRepository, groupChatAttachmentImages);
      when(
        () => attachmentsRepository.statusStream(
          attachmentId: any(named: "attachmentId"),
        ),
      ).thenAnswer((_) => Stream.value(const UiAttachmentStatus.completed()));
    });

    List<SingleChildWidget> buildProviders() => [
      RepositoryProvider<AttachmentsRepository>.value(
        value: attachmentsRepository,
      ),
      RepositoryProvider<chats_repository.ChatsRepository>.value(
        value: FakeChatsRepository(chats),
      ),
      BlocProvider<NavigationCubit>.value(value: navigationCubit),
      BlocProvider<UserCubit>.value(value: userCubit),
      BlocProvider<UsersCubit>.value(value: contactsCubit),
      BlocProvider<ChatDetailsCubit>.value(value: chatDetailsCubit),
      BlocProvider<MessageListCubit>.value(value: messageListCubit),
      BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
    ];

    const chatScreen = ChatScreenView(
      createMessageCubit: createMockMessageCubit,
    );

    Widget buildSubject(ProductShotDevice device) => buildProductShotSubject(
      providers: buildProviders(),
      shot: ProductShot(
        size: _canvasSizeFor(device.platform),
        backgroundColor: backgroundColor,
        titleColor: titleColor,
        subtitleColor: subtitleColor,
        title: title,
        subtitle: subtitle,
        frameColor: frameColor,
        device: device,
        child: _chatShot(device.platform, chatScreen),
      ),
    );

    Widget buildFramelessSubject(DeviceInfo device, TargetPlatform platform) =>
        buildProductShotSubject(
          providers: buildProviders(),
          frameless: true,
          shot: DeviceFrame(
            device: device,
            screen: _chatShot(platform, chatScreen),
          ),
        );

    for (final productShotInfo in productShotsMatrix) {
      final device = productShotInfo.targetPlatform.device;
      final deviceInfo = device.deviceInfo;
      final identifier = device.identifier;

      testProductShot(
        "Group chat (${deviceInfo.name})",
        productShotInfo: productShotInfo,
        deviceInfo: deviceInfo,
        physicalSizeOverride: _canvasSizeFor(productShotInfo.targetPlatform),
        (tester) async {
          await tester.pumpWidget(buildSubject(device));
          await _precacheImages(tester);
          await tester.pumpAndSettle();

          await expectLater(
            find.byType(ProductShot),
            // Do not change the ios/android file names, as they are
            // referenced in stores/ios/en-US/screenshots and
            // stores/android/metadata/en-US/images/phone-screenshots
            matchesGoldenFile(_golden("group_chat.$identifier.png")),
          );
        },
      );

      // Build the product shot without marketing chrome
      testProductShot(
        "Group chat (${deviceInfo.name})",
        productShotInfo: productShotInfo,
        deviceInfo: deviceInfo,
        (tester) async {
          await tester.pumpWidget(
            buildFramelessSubject(deviceInfo, productShotInfo.targetPlatform),
          );
          await _precacheImages(tester);
          await tester.pumpAndSettle();

          await expectLater(
            find.byType(DeviceFrame),
            matchesGoldenFile(_golden("group_chat.$identifier.png")),
          );
        },
      );
    }
  });
}

/// Stubs each attachment id to its own image, since the mocked repository
/// otherwise can't tell which attachment is being requested.
void _stubAttachments(
  MockAttachmentsRepository attachmentsRepository,
  Map<AttachmentId, ImageData> images,
) {
  for (final entry in images.entries) {
    when(
      () => attachmentsRepository.loadImageAttachment(
        attachmentId: entry.key,
        retryDownloadIfFailed: false,
        chunkEventCallback: any(named: "chunkEventCallback"),
      ),
    ).thenAnswer(
      (_) => Future.value(
        LoadedImageAttachment(bytes: entry.value.data, isAnimated: false),
      ),
    );
  }
}

void testProductShot(
  String description,
  WidgetTesterCallback callback, {
  required ProductShotInfo productShotInfo,
  required DeviceInfo deviceInfo,
  Size? physicalSizeOverride,
}) async {
  testWidgets(
    description,
    (tester) async {
      debugDisableShadows = false;

      tester.view.physicalSize = physicalSizeOverride ?? deviceInfo.frameSize;
      tester.view.devicePixelRatio = 1.0;
      addTearDown(() {
        tester.view.resetPhysicalSize();
        tester.view.resetDevicePixelRatio();
      });

      try {
        await callback(tester);
      } finally {
        debugDisableShadows = true;
      }
    },
    skip:
        !_renderAllPlatforms &&
        Platform.operatingSystem != productShotInfo.hostPlatform,
    variant: TargetPlatformVariant.only(productShotInfo.targetPlatform),
  );
}

/// Preload all images in the widget tree.
///
/// This is necessary in tests, otherwise the images will not be rendered.
///
/// Will be called inside `tester.runAsync`. Otherwise, `precacheImage` will
/// never complete due to fake-async.
Future<void> _precacheImages(WidgetTester tester) async {
  // [AttachmentImage] inserts its [Image] child only after an async
  // classification step resolves; settle once so the tree contains every
  // [Image] we need to precache.
  await tester.pumpAndSettle();
  await tester.runAsync(() async {
    final elements = tester.elementList(find.byType(DecoratedBox));
    for (Element element in elements) {
      final widget = element.widget as DecoratedBox;
      final image = switch (widget.decoration) {
        BoxDecoration(:final image) => image,
        ShapeDecoration(:final image) => image,
        _ => null,
      };
      if (image != null) {
        await precacheImage(image.image, element);
      }
    }

    final attachmentElements = tester.elementList(find.byType(Image));
    for (Element element in attachmentElements) {
      final image = element.widget as Image;
      await precacheImage(image.image, element);
    }
  });
}
