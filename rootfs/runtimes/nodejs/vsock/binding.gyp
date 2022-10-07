{
  "targets": [
    {
      "target_name": "vsock",
      "sources": [ "vsock.cc" ],
      "include_dirs": ["<!@(node -p \"require('node-addon-api').include\")"],
      "defines": [ 'NAPI_DISABLE_CPP_EXCEPTIONS' ]
    }
  ]
}
