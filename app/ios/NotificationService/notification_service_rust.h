//
//  notification_service_rust.h
//  NotificationService
//

#ifndef notification_service_rust_h
#define notification_service_rust_h

#include <stdint.h>

char* process_new_messages(const char* content);
void free_string(char* content);
void init_background_logger(const char* log_file_path);
void rust_log(uint8_t level, const char* message);

#endif /* notification_service_rust_h */
