// 将第 47 行：
let result = auth.login("non_existent", "password", None).await;

// 改为：
let result = auth.login("non_existent", "password", None, None).await;