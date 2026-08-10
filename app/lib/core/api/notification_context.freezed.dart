// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'notification_context.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$NotificationPolicy {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is NotificationPolicy);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'NotificationPolicy()';
}


}

/// @nodoc
class $NotificationPolicyCopyWith<$Res>  {
$NotificationPolicyCopyWith(NotificationPolicy _, $Res Function(NotificationPolicy) __);
}



/// @nodoc


class NotificationPolicy_SuppressAll extends NotificationPolicy {
  const NotificationPolicy_SuppressAll(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is NotificationPolicy_SuppressAll);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'NotificationPolicy.suppressAll()';
}


}




/// @nodoc


class NotificationPolicy_AllowAll extends NotificationPolicy {
  const NotificationPolicy_AllowAll(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is NotificationPolicy_AllowAll);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'NotificationPolicy.allowAll()';
}


}




/// @nodoc


class NotificationPolicy_SuppressChat extends NotificationPolicy {
  const NotificationPolicy_SuppressChat({required this.chatId}): super._();
  

 final  ChatId chatId;

/// Create a copy of NotificationPolicy
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$NotificationPolicy_SuppressChatCopyWith<NotificationPolicy_SuppressChat> get copyWith => _$NotificationPolicy_SuppressChatCopyWithImpl<NotificationPolicy_SuppressChat>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is NotificationPolicy_SuppressChat&&(identical(other.chatId, chatId) || other.chatId == chatId));
}


@override
int get hashCode => Object.hash(runtimeType,chatId);

@override
String toString() {
  return 'NotificationPolicy.suppressChat(chatId: $chatId)';
}


}

/// @nodoc
abstract mixin class $NotificationPolicy_SuppressChatCopyWith<$Res> implements $NotificationPolicyCopyWith<$Res> {
  factory $NotificationPolicy_SuppressChatCopyWith(NotificationPolicy_SuppressChat value, $Res Function(NotificationPolicy_SuppressChat) _then) = _$NotificationPolicy_SuppressChatCopyWithImpl;
@useResult
$Res call({
 ChatId chatId
});




}
/// @nodoc
class _$NotificationPolicy_SuppressChatCopyWithImpl<$Res>
    implements $NotificationPolicy_SuppressChatCopyWith<$Res> {
  _$NotificationPolicy_SuppressChatCopyWithImpl(this._self, this._then);

  final NotificationPolicy_SuppressChat _self;
  final $Res Function(NotificationPolicy_SuppressChat) _then;

/// Create a copy of NotificationPolicy
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? chatId = null,}) {
  return _then(NotificationPolicy_SuppressChat(
chatId: null == chatId ? _self.chatId : chatId // ignore: cast_nullable_to_non_nullable
as ChatId,
  ));
}


}

// dart format on
