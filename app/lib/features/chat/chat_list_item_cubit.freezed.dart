// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'chat_list_item_cubit.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$ChatListItemState {

 UiChatDetails get chat;
/// Create a copy of ChatListItemState
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$ChatListItemStateCopyWith<ChatListItemState> get copyWith => _$ChatListItemStateCopyWithImpl<ChatListItemState>(this as ChatListItemState, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is ChatListItemState&&(identical(other.chat, chat) || other.chat == chat));
}


@override
int get hashCode => Object.hash(runtimeType,chat);

@override
String toString() {
  return 'ChatListItemState(chat: $chat)';
}


}

/// @nodoc
abstract mixin class $ChatListItemStateCopyWith<$Res>  {
  factory $ChatListItemStateCopyWith(ChatListItemState value, $Res Function(ChatListItemState) _then) = _$ChatListItemStateCopyWithImpl;
@useResult
$Res call({
 UiChatDetails chat
});




}
/// @nodoc
class _$ChatListItemStateCopyWithImpl<$Res>
    implements $ChatListItemStateCopyWith<$Res> {
  _$ChatListItemStateCopyWithImpl(this._self, this._then);

  final ChatListItemState _self;
  final $Res Function(ChatListItemState) _then;

/// Create a copy of ChatListItemState
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? chat = null,}) {
  return _then(_self.copyWith(
chat: null == chat ? _self.chat : chat // ignore: cast_nullable_to_non_nullable
as UiChatDetails,
  ));
}

}



/// @nodoc


class _ChatListItemState implements ChatListItemState {
  const _ChatListItemState({required this.chat});
  

@override final  UiChatDetails chat;

/// Create a copy of ChatListItemState
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$ChatListItemStateCopyWith<_ChatListItemState> get copyWith => __$ChatListItemStateCopyWithImpl<_ChatListItemState>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _ChatListItemState&&(identical(other.chat, chat) || other.chat == chat));
}


@override
int get hashCode => Object.hash(runtimeType,chat);

@override
String toString() {
  return 'ChatListItemState(chat: $chat)';
}


}

/// @nodoc
abstract mixin class _$ChatListItemStateCopyWith<$Res> implements $ChatListItemStateCopyWith<$Res> {
  factory _$ChatListItemStateCopyWith(_ChatListItemState value, $Res Function(_ChatListItemState) _then) = __$ChatListItemStateCopyWithImpl;
@override @useResult
$Res call({
 UiChatDetails chat
});




}
/// @nodoc
class __$ChatListItemStateCopyWithImpl<$Res>
    implements _$ChatListItemStateCopyWith<$Res> {
  __$ChatListItemStateCopyWithImpl(this._self, this._then);

  final _ChatListItemState _self;
  final $Res Function(_ChatListItemState) _then;

/// Create a copy of ChatListItemState
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? chat = null,}) {
  return _then(_ChatListItemState(
chat: null == chat ? _self.chat : chat // ignore: cast_nullable_to_non_nullable
as UiChatDetails,
  ));
}


}

// dart format on
