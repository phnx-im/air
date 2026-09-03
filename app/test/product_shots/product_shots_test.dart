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

/// Every host/target combination a product shot is recorded for.
const _productShotsMatrix = [
  ProductShotInfo(hostPlatform: 'macos', targetPlatform: TargetPlatform.iOS),
  ProductShotInfo(hostPlatform: 'macos', targetPlatform: TargetPlatform.macOS),
  ProductShotInfo(
    hostPlatform: 'linux',
    targetPlatform: TargetPlatform.android,
  ),
  ProductShotInfo(hostPlatform: 'linux', targetPlatform: TargetPlatform.linux),
  ProductShotInfo(
    hostPlatform: 'windows',
    targetPlatform: TargetPlatform.windows,
  ),
];

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
Widget _buildProductShotSubject({
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

/// Everything one product shot subject (chat list, a private chat, a group
/// chat, ...) needs: its marketing copy/colors, the providers backing it, and
/// the screen to depict for a given target platform.
class _ProductShotSpec {
  const _ProductShotSpec({
    required this.goldenPrefix,
    required this.backgroundColor,
    required this.titleColor,
    required this.subtitleColor,
    required this.frameColor,
    required this.title,
    required this.subtitle,
    required this.buildProviders,
    required this.buildScreen,
  });

  final String goldenPrefix;
  final Color backgroundColor;
  final Color titleColor;
  final Color subtitleColor;
  final Color frameColor;
  final String title;
  final String subtitle;

  /// Builds fresh mocks/providers for a single test run.
  final List<SingleChildWidget> Function() buildProviders;

  final Widget Function(TargetPlatform platform) buildScreen;
}

/// Registers the marketing and frameless golden tests for [spec] across
/// [_productShotsMatrix].
void _testProductShots(String groupName, _ProductShotSpec spec) {
  group(groupName, () {
    for (final productShotInfo in _productShotsMatrix) {
      final device = productShotInfo.targetPlatform.device;
      final deviceInfo = device.deviceInfo;
      final identifier = device.identifier;

      testProductShot(
        "$groupName (${deviceInfo.name})",
        productShotInfo: productShotInfo,
        deviceInfo: deviceInfo,
        physicalSizeOverride: _canvasSizeFor(productShotInfo.targetPlatform),
        (tester) async {
          await tester.pumpWidget(
            _buildProductShotSubject(
              providers: spec.buildProviders(),
              shot: ProductShot(
                size: _canvasSizeFor(productShotInfo.targetPlatform),
                backgroundColor: spec.backgroundColor,
                titleColor: spec.titleColor,
                subtitleColor: spec.subtitleColor,
                title: spec.title,
                subtitle: spec.subtitle,
                frameColor: spec.frameColor,
                device: device,
                child: spec.buildScreen(productShotInfo.targetPlatform),
              ),
            ),
          );
          await _precacheImages(tester);
          await tester.pumpAndSettle();

          await expectLater(
            find.byType(ProductShot),
            // Do not change the ios/android file names, as they are
            // referenced in stores/ios/en-US/screenshots and
            // stores/android/metadata/en-US/images/phone-screenshots
            matchesGoldenFile(_golden("${spec.goldenPrefix}.$identifier.png")),
          );
        },
      );

      // Build the product shot without marketing chrome
      testProductShot(
        "$groupName (${deviceInfo.name}, frameless)",
        productShotInfo: productShotInfo,
        deviceInfo: deviceInfo,
        (tester) async {
          await tester.pumpWidget(
            _buildProductShotSubject(
              providers: spec.buildProviders(),
              frameless: true,
              shot: DeviceFrame(
                device: deviceInfo,
                screen: spec.buildScreen(productShotInfo.targetPlatform),
              ),
            ),
          );
          await _precacheImages(tester);
          await tester.pumpAndSettle();

          await expectLater(
            find.byType(DeviceFrame),
            matchesGoldenFile(
              _golden("${spec.goldenPrefix}.$identifier.frameless.png"),
            ),
          );
        },
      );
    }
  });
}

_ProductShotSpec _chatListSpec() => _ProductShotSpec(
  goldenPrefix: 'chat_list',
  backgroundColor: Primitive.neutral(NeutralShade.s100),
  titleColor: Primitive.neutral(NeutralShade.s800),
  subtitleColor: Primitive.neutral(NeutralShade.s600),
  frameColor: Primitive.neutral(NeutralShade.s300),
  title: 'Secure messaging\nfor everyone.',
  subtitle: 'Everything in Air is\nend-to-end encrypted.',
  buildProviders: () {
    final navigationCubit = MockNavigationCubit();
    final userCubit = MockUserCubit();
    final usersCubit = MockUsersCubit();
    final userSettingsCubit = MockUserSettingsCubit();

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

    return [
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
  },
  buildScreen: (_) => const Stack(
    children: [
      Positioned.fill(child: ChatListView(scaffold: true)),
      Positioned(left: 0, right: 0, bottom: 0, child: AppTabBar()),
    ],
  ),
);

/// A private chat and a group chat only differ in which conversation they
/// depict, so they share this spec and just plug in their own fixtures.
_ProductShotSpec _chatSpec({
  required String goldenPrefix,
  required Color backgroundColor,
  required Color titleColor,
  required Color subtitleColor,
  required Color frameColor,
  required String title,
  required String subtitle,
  required UiChatDetails chat,
  required List<UiUserId> members,
  required List<UiChatMessage> messages,
  required Map<AttachmentId, ImageData> attachmentImages,
}) => _ProductShotSpec(
  goldenPrefix: goldenPrefix,
  backgroundColor: backgroundColor,
  titleColor: titleColor,
  subtitleColor: subtitleColor,
  frameColor: frameColor,
  title: title,
  subtitle: subtitle,
  buildProviders: () {
    final navigationCubit = MockNavigationCubit();
    final userCubit = MockUserCubit();
    final contactsCubit = MockUsersCubit();
    final chatDetailsCubit = MockChatDetailsCubit();
    final messageListCubit = MockMessageListCubit();
    final userSettingsCubit = MockUserSettingsCubit();
    final attachmentsRepository = MockAttachmentsRepository();

    when(() => navigationCubit.state).thenReturn(
      NavigationState.home(home: HomeNavigationState(chatId: chat.id)),
    );
    when(() => userCubit.state).thenReturn(MockUiUser(id: ownIdx));
    when(
      () => contactsCubit.state,
    ).thenReturn(MockUsersState(profiles: userProfiles));
    when(
      () => chatDetailsCubit.state,
    ).thenReturn(ChatDetailsState(chat: chat, members: members));
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
    messageListCubit.setState(messages);
    _stubAttachments(attachmentsRepository, attachmentImages);
    when(
      () => attachmentsRepository.statusStream(
        attachmentId: any(named: "attachmentId"),
      ),
    ).thenAnswer((_) => Stream.value(const UiAttachmentStatus.completed()));

    return [
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
  },
  buildScreen: (platform) => _chatShot(
    platform,
    const ChatScreenView(createMessageCubit: createMockMessageCubit),
  ),
);

_ProductShotSpec _privateChatSpec() => _chatSpec(
  goldenPrefix: 'private_chat',
  backgroundColor: Primitive.chromatic(Hue.orange, Shade.s50),
  titleColor: Primitive.chromatic(Hue.orange, Shade.s800),
  subtitleColor: Primitive.chromatic(Hue.orange, Shade.s600),
  frameColor: Primitive.chromatic(Hue.orange, Shade.s300),
  title: 'Connect with friends.',
  subtitle: 'Send messages in private chats.',
  chat: privateChat,
  members: privateChatMembers,
  messages: privateChatMessages,
  attachmentImages: privateChatAttachmentImages,
);

_ProductShotSpec _groupChatSpec() => _chatSpec(
  goldenPrefix: 'group_chat',
  backgroundColor: Primitive.chromatic(Hue.blue, Shade.s50),
  titleColor: Primitive.chromatic(Hue.blue, Shade.s800),
  subtitleColor: Primitive.chromatic(Hue.blue, Shade.s600),
  frameColor: Primitive.chromatic(Hue.blue, Shade.s300),
  title: 'Create group chats.',
  subtitle: 'Message with multiple people.',
  chat: groupChat,
  members: groupChatMembers,
  messages: groupChatMessages,
  attachmentImages: groupChatAttachmentImages,
);

void main() {
  setUpAll(() {
    registerFallbackValue(0.messageId());
    registerFallbackValue(0.userId());
    registerFallbackValue(0.attachmentId());
  });

  _testProductShots('Chat List', _chatListSpec());
  _testProductShots('Private Chat', _privateChatSpec());
  _testProductShots('Group Chat', _groupChatSpec());
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

/// [productShotInfo.hostPlatform] is the OS that records the shot, not the
/// device it depicts: the iOS shots are recorded on macOS because that is
/// where the San Francisco system font is available.
/// [productShotInfo.targetPlatform] is the depicted platform, which drives
/// the typescale and the device type independently of the host.
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
