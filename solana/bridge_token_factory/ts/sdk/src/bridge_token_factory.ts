/**
 * Program IDL in camelCase format in order to be used in JS/TS.
 *
 * Note that this is only a type helper and is not the actual IDL. The original
 * IDL can be found at `target/idl/bridge_token_factory.json`.
 */
export type BridgeTokenFactory = {
  "address": "3ZtEZ8xABFbUr4c1FVpXbQiVdqv4vwhvfCc8HMmhEeua",
  "metadata": {
    "name": "bridgeTokenFactory",
    "version": "0.1.0",
    "spec": "0.1.0",
    "description": "Created with Anchor"
  },
  "instructions": [
    {
      "name": "deployToken",
      "discriminator": [
        144,
        104,
        20,
        192,
        18,
        112,
        224,
        140
      ],
      "accounts": [
        {
          "name": "authority",
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  97,
                  117,
                  116,
                  104,
                  111,
                  114,
                  105,
                  116,
                  121
                ]
              }
            ]
          }
        },
        {
          "name": "mint",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  119,
                  114,
                  97,
                  112,
                  112,
                  101,
                  100,
                  95,
                  109,
                  105,
                  110,
                  116
                ]
              },
              {
                "kind": "arg",
                "path": "data.payload.token"
              }
            ]
          }
        },
        {
          "name": "metadata",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  109,
                  101,
                  116,
                  97,
                  100,
                  97,
                  116,
                  97
                ]
              },
              {
                "kind": "const",
                "value": [
                  11,
                  112,
                  101,
                  177,
                  227,
                  209,
                  124,
                  69,
                  56,
                  157,
                  82,
                  127,
                  107,
                  4,
                  195,
                  205,
                  88,
                  184,
                  108,
                  115,
                  26,
                  160,
                  253,
                  181,
                  73,
                  182,
                  209,
                  188,
                  3,
                  248,
                  41,
                  70
                ]
              },
              {
                "kind": "account",
                "path": "mint"
              }
            ],
            "program": {
              "kind": "const",
              "value": [
                11,
                112,
                101,
                177,
                227,
                209,
                124,
                69,
                56,
                157,
                82,
                127,
                107,
                4,
                195,
                205,
                88,
                184,
                108,
                115,
                26,
                160,
                253,
                181,
                73,
                182,
                209,
                188,
                3,
                248,
                41,
                70
              ]
            }
          }
        },
        {
          "name": "wormhole",
          "accounts": [
            {
              "name": "config",
              "docs": [
                "Used as an emitter"
              ],
              "pda": {
                "seeds": [
                  {
                    "kind": "const",
                    "value": [
                      99,
                      111,
                      110,
                      102,
                      105,
                      103
                    ]
                  }
                ]
              }
            },
            {
              "name": "bridge",
              "docs": [
                "Wormhole bridge data account (a.k.a. its config).",
                "[`wormhole::post_message`] requires this account be mutable."
              ],
              "writable": true,
              "pda": {
                "seeds": [
                  {
                    "kind": "const",
                    "value": [
                      66,
                      114,
                      105,
                      100,
                      103,
                      101
                    ]
                  }
                ]
              }
            },
            {
              "name": "feeCollector",
              "docs": [
                "Wormhole fee collector account, which requires lamports before the",
                "program can post a message (if there is a fee).",
                "[`wormhole::post_message`] requires this account be mutable."
              ],
              "writable": true,
              "pda": {
                "seeds": [
                  {
                    "kind": "const",
                    "value": [
                      102,
                      101,
                      101,
                      95,
                      99,
                      111,
                      108,
                      108,
                      101,
                      99,
                      116,
                      111,
                      114
                    ]
                  }
                ]
              }
            },
            {
              "name": "sequence",
              "docs": [
                "message is posted, so it needs to be an [UncheckedAccount] for the",
                "[`initialize`](crate::initialize) instruction.",
                "[`wormhole::post_message`] requires this account be mutable."
              ],
              "writable": true,
              "pda": {
                "seeds": [
                  {
                    "kind": "const",
                    "value": [
                      83,
                      101,
                      113,
                      117,
                      101,
                      110,
                      99,
                      101
                    ]
                  },
                  {
                    "kind": "account",
                    "path": "config"
                  }
                ]
              }
            },
            {
              "name": "message",
              "docs": [
                "account be mutable."
              ],
              "writable": true,
              "signer": true
            },
            {
              "name": "payer",
              "writable": true,
              "signer": true
            },
            {
              "name": "clock",
              "address": "SysvarC1ock11111111111111111111111111111111"
            },
            {
              "name": "rent",
              "address": "SysvarRent111111111111111111111111111111111"
            },
            {
              "name": "wormholeProgram",
              "docs": [
                "Wormhole program."
              ],
              "address": "worm2ZoG2kUd4vFXhvjh93UUH596ayRfgQ2MgjNMTth"
            },
            {
              "name": "systemProgram",
              "address": "11111111111111111111111111111111"
            }
          ]
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        },
        {
          "name": "tokenProgram",
          "address": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        },
        {
          "name": "tokenMetadataProgram",
          "address": "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"
        }
      ],
      "args": [
        {
          "name": "data",
          "type": {
            "defined": {
              "name": "signedPayload",
              "generics": [
                {
                  "kind": "type",
                  "type": {
                    "defined": {
                      "name": "deployTokenPayload"
                    }
                  }
                }
              ]
            }
          }
        }
      ]
    },
    {
      "name": "finalizeTransferBridged",
      "discriminator": [
        9,
        113,
        68,
        220,
        238,
        32,
        44,
        13
      ],
      "accounts": [
        {
          "name": "config",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        },
        {
          "name": "usedNonces",
          "writable": true
        },
        {
          "name": "recipient"
        },
        {
          "name": "authority",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  97,
                  117,
                  116,
                  104,
                  111,
                  114,
                  105,
                  116,
                  121
                ]
              }
            ]
          }
        },
        {
          "name": "mint",
          "writable": true
        },
        {
          "name": "tokenAccount",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "account",
                "path": "recipient"
              },
              {
                "kind": "const",
                "value": [
                  6,
                  221,
                  246,
                  225,
                  215,
                  101,
                  161,
                  147,
                  217,
                  203,
                  225,
                  70,
                  206,
                  235,
                  121,
                  172,
                  28,
                  180,
                  133,
                  237,
                  95,
                  91,
                  55,
                  145,
                  58,
                  140,
                  245,
                  133,
                  126,
                  255,
                  0,
                  169
                ]
              },
              {
                "kind": "account",
                "path": "mint"
              }
            ],
            "program": {
              "kind": "const",
              "value": [
                140,
                151,
                37,
                143,
                78,
                36,
                137,
                241,
                187,
                61,
                16,
                41,
                20,
                142,
                13,
                131,
                11,
                90,
                19,
                153,
                218,
                255,
                16,
                132,
                4,
                142,
                123,
                216,
                219,
                233,
                248,
                89
              ]
            }
          }
        },
        {
          "name": "wormhole",
          "accounts": [
            {
              "name": "config",
              "docs": [
                "Used as an emitter"
              ],
              "pda": {
                "seeds": [
                  {
                    "kind": "const",
                    "value": [
                      99,
                      111,
                      110,
                      102,
                      105,
                      103
                    ]
                  }
                ]
              }
            },
            {
              "name": "bridge",
              "docs": [
                "Wormhole bridge data account (a.k.a. its config).",
                "[`wormhole::post_message`] requires this account be mutable."
              ],
              "writable": true,
              "pda": {
                "seeds": [
                  {
                    "kind": "const",
                    "value": [
                      66,
                      114,
                      105,
                      100,
                      103,
                      101
                    ]
                  }
                ]
              }
            },
            {
              "name": "feeCollector",
              "docs": [
                "Wormhole fee collector account, which requires lamports before the",
                "program can post a message (if there is a fee).",
                "[`wormhole::post_message`] requires this account be mutable."
              ],
              "writable": true,
              "pda": {
                "seeds": [
                  {
                    "kind": "const",
                    "value": [
                      102,
                      101,
                      101,
                      95,
                      99,
                      111,
                      108,
                      108,
                      101,
                      99,
                      116,
                      111,
                      114
                    ]
                  }
                ]
              }
            },
            {
              "name": "sequence",
              "docs": [
                "message is posted, so it needs to be an [UncheckedAccount] for the",
                "[`initialize`](crate::initialize) instruction.",
                "[`wormhole::post_message`] requires this account be mutable."
              ],
              "writable": true,
              "pda": {
                "seeds": [
                  {
                    "kind": "const",
                    "value": [
                      83,
                      101,
                      113,
                      117,
                      101,
                      110,
                      99,
                      101
                    ]
                  },
                  {
                    "kind": "account",
                    "path": "config"
                  }
                ]
              }
            },
            {
              "name": "message",
              "docs": [
                "account be mutable."
              ],
              "writable": true,
              "signer": true
            },
            {
              "name": "payer",
              "writable": true,
              "signer": true
            },
            {
              "name": "clock",
              "address": "SysvarC1ock11111111111111111111111111111111"
            },
            {
              "name": "rent",
              "address": "SysvarRent111111111111111111111111111111111"
            },
            {
              "name": "wormholeProgram",
              "docs": [
                "Wormhole program."
              ],
              "address": "worm2ZoG2kUd4vFXhvjh93UUH596ayRfgQ2MgjNMTth"
            },
            {
              "name": "systemProgram",
              "address": "11111111111111111111111111111111"
            }
          ]
        },
        {
          "name": "associatedTokenProgram",
          "address": "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        },
        {
          "name": "tokenProgram",
          "address": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        }
      ],
      "args": [
        {
          "name": "data",
          "type": {
            "defined": {
              "name": "signedPayload",
              "generics": [
                {
                  "kind": "type",
                  "type": {
                    "defined": {
                      "name": "finalizeTransferPayload"
                    }
                  }
                }
              ]
            }
          }
        }
      ]
    },
    {
      "name": "finalizeTransferNative",
      "discriminator": [
        27,
        208,
        189,
        73,
        113,
        171,
        160,
        204
      ],
      "accounts": [
        {
          "name": "config",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        },
        {
          "name": "usedNonces",
          "writable": true
        },
        {
          "name": "authority",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  97,
                  117,
                  116,
                  104,
                  111,
                  114,
                  105,
                  116,
                  121
                ]
              }
            ]
          }
        },
        {
          "name": "recipient"
        },
        {
          "name": "mint"
        },
        {
          "name": "vault",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  118,
                  97,
                  117,
                  108,
                  116
                ]
              },
              {
                "kind": "account",
                "path": "mint"
              }
            ]
          }
        },
        {
          "name": "tokenAccount",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "account",
                "path": "recipient"
              },
              {
                "kind": "const",
                "value": [
                  6,
                  221,
                  246,
                  225,
                  215,
                  101,
                  161,
                  147,
                  217,
                  203,
                  225,
                  70,
                  206,
                  235,
                  121,
                  172,
                  28,
                  180,
                  133,
                  237,
                  95,
                  91,
                  55,
                  145,
                  58,
                  140,
                  245,
                  133,
                  126,
                  255,
                  0,
                  169
                ]
              },
              {
                "kind": "account",
                "path": "mint"
              }
            ],
            "program": {
              "kind": "const",
              "value": [
                140,
                151,
                37,
                143,
                78,
                36,
                137,
                241,
                187,
                61,
                16,
                41,
                20,
                142,
                13,
                131,
                11,
                90,
                19,
                153,
                218,
                255,
                16,
                132,
                4,
                142,
                123,
                216,
                219,
                233,
                248,
                89
              ]
            }
          }
        },
        {
          "name": "wormhole",
          "accounts": [
            {
              "name": "config",
              "docs": [
                "Used as an emitter"
              ],
              "pda": {
                "seeds": [
                  {
                    "kind": "const",
                    "value": [
                      99,
                      111,
                      110,
                      102,
                      105,
                      103
                    ]
                  }
                ]
              }
            },
            {
              "name": "bridge",
              "docs": [
                "Wormhole bridge data account (a.k.a. its config).",
                "[`wormhole::post_message`] requires this account be mutable."
              ],
              "writable": true,
              "pda": {
                "seeds": [
                  {
                    "kind": "const",
                    "value": [
                      66,
                      114,
                      105,
                      100,
                      103,
                      101
                    ]
                  }
                ]
              }
            },
            {
              "name": "feeCollector",
              "docs": [
                "Wormhole fee collector account, which requires lamports before the",
                "program can post a message (if there is a fee).",
                "[`wormhole::post_message`] requires this account be mutable."
              ],
              "writable": true,
              "pda": {
                "seeds": [
                  {
                    "kind": "const",
                    "value": [
                      102,
                      101,
                      101,
                      95,
                      99,
                      111,
                      108,
                      108,
                      101,
                      99,
                      116,
                      111,
                      114
                    ]
                  }
                ]
              }
            },
            {
              "name": "sequence",
              "docs": [
                "message is posted, so it needs to be an [UncheckedAccount] for the",
                "[`initialize`](crate::initialize) instruction.",
                "[`wormhole::post_message`] requires this account be mutable."
              ],
              "writable": true,
              "pda": {
                "seeds": [
                  {
                    "kind": "const",
                    "value": [
                      83,
                      101,
                      113,
                      117,
                      101,
                      110,
                      99,
                      101
                    ]
                  },
                  {
                    "kind": "account",
                    "path": "config"
                  }
                ]
              }
            },
            {
              "name": "message",
              "docs": [
                "account be mutable."
              ],
              "writable": true,
              "signer": true
            },
            {
              "name": "payer",
              "writable": true,
              "signer": true
            },
            {
              "name": "clock",
              "address": "SysvarC1ock11111111111111111111111111111111"
            },
            {
              "name": "rent",
              "address": "SysvarRent111111111111111111111111111111111"
            },
            {
              "name": "wormholeProgram",
              "docs": [
                "Wormhole program."
              ],
              "address": "worm2ZoG2kUd4vFXhvjh93UUH596ayRfgQ2MgjNMTth"
            },
            {
              "name": "systemProgram",
              "address": "11111111111111111111111111111111"
            }
          ]
        },
        {
          "name": "associatedTokenProgram",
          "address": "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        },
        {
          "name": "tokenProgram"
        }
      ],
      "args": [
        {
          "name": "data",
          "type": {
            "defined": {
              "name": "signedPayload",
              "generics": [
                {
                  "kind": "type",
                  "type": {
                    "defined": {
                      "name": "finalizeTransferPayload"
                    }
                  }
                }
              ]
            }
          }
        }
      ]
    },
    {
      "name": "initTransferBridged",
      "discriminator": [
        102,
        4,
        222,
        127,
        222,
        254,
        91,
        156
      ],
      "accounts": [
        {
          "name": "authority",
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  97,
                  117,
                  116,
                  104,
                  111,
                  114,
                  105,
                  116,
                  121
                ]
              }
            ]
          }
        },
        {
          "name": "mint",
          "writable": true
        },
        {
          "name": "from",
          "writable": true
        },
        {
          "name": "user",
          "signer": true
        },
        {
          "name": "wormhole",
          "accounts": [
            {
              "name": "config",
              "docs": [
                "Used as an emitter"
              ],
              "pda": {
                "seeds": [
                  {
                    "kind": "const",
                    "value": [
                      99,
                      111,
                      110,
                      102,
                      105,
                      103
                    ]
                  }
                ]
              }
            },
            {
              "name": "bridge",
              "docs": [
                "Wormhole bridge data account (a.k.a. its config).",
                "[`wormhole::post_message`] requires this account be mutable."
              ],
              "writable": true,
              "pda": {
                "seeds": [
                  {
                    "kind": "const",
                    "value": [
                      66,
                      114,
                      105,
                      100,
                      103,
                      101
                    ]
                  }
                ]
              }
            },
            {
              "name": "feeCollector",
              "docs": [
                "Wormhole fee collector account, which requires lamports before the",
                "program can post a message (if there is a fee).",
                "[`wormhole::post_message`] requires this account be mutable."
              ],
              "writable": true,
              "pda": {
                "seeds": [
                  {
                    "kind": "const",
                    "value": [
                      102,
                      101,
                      101,
                      95,
                      99,
                      111,
                      108,
                      108,
                      101,
                      99,
                      116,
                      111,
                      114
                    ]
                  }
                ]
              }
            },
            {
              "name": "sequence",
              "docs": [
                "message is posted, so it needs to be an [UncheckedAccount] for the",
                "[`initialize`](crate::initialize) instruction.",
                "[`wormhole::post_message`] requires this account be mutable."
              ],
              "writable": true,
              "pda": {
                "seeds": [
                  {
                    "kind": "const",
                    "value": [
                      83,
                      101,
                      113,
                      117,
                      101,
                      110,
                      99,
                      101
                    ]
                  },
                  {
                    "kind": "account",
                    "path": "config"
                  }
                ]
              }
            },
            {
              "name": "message",
              "docs": [
                "account be mutable."
              ],
              "writable": true,
              "signer": true
            },
            {
              "name": "payer",
              "writable": true,
              "signer": true
            },
            {
              "name": "clock",
              "address": "SysvarC1ock11111111111111111111111111111111"
            },
            {
              "name": "rent",
              "address": "SysvarRent111111111111111111111111111111111"
            },
            {
              "name": "wormholeProgram",
              "docs": [
                "Wormhole program."
              ],
              "address": "worm2ZoG2kUd4vFXhvjh93UUH596ayRfgQ2MgjNMTth"
            },
            {
              "name": "systemProgram",
              "address": "11111111111111111111111111111111"
            }
          ]
        },
        {
          "name": "tokenProgram",
          "address": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        }
      ],
      "args": [
        {
          "name": "payload",
          "type": {
            "defined": {
              "name": "initTransferPayload"
            }
          }
        }
      ]
    },
    {
      "name": "initTransferNative",
      "discriminator": [
        253,
        5,
        175,
        189,
        176,
        62,
        114,
        77
      ],
      "accounts": [
        {
          "name": "authority",
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  97,
                  117,
                  116,
                  104,
                  111,
                  114,
                  105,
                  116,
                  121
                ]
              }
            ]
          }
        },
        {
          "name": "mint"
        },
        {
          "name": "from",
          "writable": true
        },
        {
          "name": "vault",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  118,
                  97,
                  117,
                  108,
                  116
                ]
              },
              {
                "kind": "account",
                "path": "mint"
              }
            ]
          }
        },
        {
          "name": "user",
          "signer": true
        },
        {
          "name": "wormhole",
          "accounts": [
            {
              "name": "config",
              "docs": [
                "Used as an emitter"
              ],
              "pda": {
                "seeds": [
                  {
                    "kind": "const",
                    "value": [
                      99,
                      111,
                      110,
                      102,
                      105,
                      103
                    ]
                  }
                ]
              }
            },
            {
              "name": "bridge",
              "docs": [
                "Wormhole bridge data account (a.k.a. its config).",
                "[`wormhole::post_message`] requires this account be mutable."
              ],
              "writable": true,
              "pda": {
                "seeds": [
                  {
                    "kind": "const",
                    "value": [
                      66,
                      114,
                      105,
                      100,
                      103,
                      101
                    ]
                  }
                ]
              }
            },
            {
              "name": "feeCollector",
              "docs": [
                "Wormhole fee collector account, which requires lamports before the",
                "program can post a message (if there is a fee).",
                "[`wormhole::post_message`] requires this account be mutable."
              ],
              "writable": true,
              "pda": {
                "seeds": [
                  {
                    "kind": "const",
                    "value": [
                      102,
                      101,
                      101,
                      95,
                      99,
                      111,
                      108,
                      108,
                      101,
                      99,
                      116,
                      111,
                      114
                    ]
                  }
                ]
              }
            },
            {
              "name": "sequence",
              "docs": [
                "message is posted, so it needs to be an [UncheckedAccount] for the",
                "[`initialize`](crate::initialize) instruction.",
                "[`wormhole::post_message`] requires this account be mutable."
              ],
              "writable": true,
              "pda": {
                "seeds": [
                  {
                    "kind": "const",
                    "value": [
                      83,
                      101,
                      113,
                      117,
                      101,
                      110,
                      99,
                      101
                    ]
                  },
                  {
                    "kind": "account",
                    "path": "config"
                  }
                ]
              }
            },
            {
              "name": "message",
              "docs": [
                "account be mutable."
              ],
              "writable": true,
              "signer": true
            },
            {
              "name": "payer",
              "writable": true,
              "signer": true
            },
            {
              "name": "clock",
              "address": "SysvarC1ock11111111111111111111111111111111"
            },
            {
              "name": "rent",
              "address": "SysvarRent111111111111111111111111111111111"
            },
            {
              "name": "wormholeProgram",
              "docs": [
                "Wormhole program."
              ],
              "address": "worm2ZoG2kUd4vFXhvjh93UUH596ayRfgQ2MgjNMTth"
            },
            {
              "name": "systemProgram",
              "address": "11111111111111111111111111111111"
            }
          ]
        },
        {
          "name": "tokenProgram"
        }
      ],
      "args": [
        {
          "name": "payload",
          "type": {
            "defined": {
              "name": "initTransferPayload"
            }
          }
        }
      ]
    },
    {
      "name": "initialize",
      "discriminator": [
        175,
        175,
        109,
        31,
        13,
        152,
        155,
        237
      ],
      "accounts": [
        {
          "name": "config",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        },
        {
          "name": "authority",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  97,
                  117,
                  116,
                  104,
                  111,
                  114,
                  105,
                  116,
                  121
                ]
              }
            ]
          }
        },
        {
          "name": "wormholeBridge",
          "docs": [
            "Wormhole bridge data account (a.k.a. its config).",
            "[`wormhole::post_message`] requires this account be mutable."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  66,
                  114,
                  105,
                  100,
                  103,
                  101
                ]
              }
            ]
          }
        },
        {
          "name": "wormholeFeeCollector",
          "docs": [
            "Wormhole fee collector account, which requires lamports before the",
            "program can post a message (if there is a fee).",
            "[`wormhole::post_message`] requires this account be mutable."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  102,
                  101,
                  101,
                  95,
                  99,
                  111,
                  108,
                  108,
                  101,
                  99,
                  116,
                  111,
                  114
                ]
              }
            ]
          }
        },
        {
          "name": "wormholeSequence",
          "docs": [
            "message is posted, so it needs to be an [UncheckedAccount] for the",
            "[`initialize`](crate::initialize) instruction.",
            "[`wormhole::post_message`] requires this account be mutable."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  83,
                  101,
                  113,
                  117,
                  101,
                  110,
                  99,
                  101
                ]
              },
              {
                "kind": "account",
                "path": "config"
              }
            ]
          }
        },
        {
          "name": "wormholeMessage",
          "docs": [
            "account be mutable."
          ],
          "writable": true,
          "signer": true
        },
        {
          "name": "payer",
          "writable": true,
          "signer": true
        },
        {
          "name": "clock",
          "address": "SysvarC1ock11111111111111111111111111111111"
        },
        {
          "name": "rent",
          "address": "SysvarRent111111111111111111111111111111111"
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        },
        {
          "name": "wormholeProgram",
          "address": "worm2ZoG2kUd4vFXhvjh93UUH596ayRfgQ2MgjNMTth"
        },
        {
          "name": "program",
          "signer": true,
          "address": "3ZtEZ8xABFbUr4c1FVpXbQiVdqv4vwhvfCc8HMmhEeua"
        }
      ],
      "args": [
        {
          "name": "admin",
          "type": "pubkey"
        },
        {
          "name": "derivedNearBridgeAddress",
          "type": {
            "array": [
              "u8",
              64
            ]
          }
        }
      ]
    },
    {
      "name": "registerMint",
      "discriminator": [
        242,
        43,
        74,
        162,
        217,
        214,
        191,
        171
      ],
      "accounts": [
        {
          "name": "authority",
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  97,
                  117,
                  116,
                  104,
                  111,
                  114,
                  105,
                  116,
                  121
                ]
              }
            ]
          }
        },
        {
          "name": "mint"
        },
        {
          "name": "overrideAuthority",
          "signer": true,
          "optional": true
        },
        {
          "name": "metadata",
          "optional": true
        },
        {
          "name": "vault",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  118,
                  97,
                  117,
                  108,
                  116
                ]
              },
              {
                "kind": "account",
                "path": "mint"
              }
            ]
          }
        },
        {
          "name": "wormhole",
          "accounts": [
            {
              "name": "config",
              "docs": [
                "Used as an emitter"
              ],
              "pda": {
                "seeds": [
                  {
                    "kind": "const",
                    "value": [
                      99,
                      111,
                      110,
                      102,
                      105,
                      103
                    ]
                  }
                ]
              }
            },
            {
              "name": "bridge",
              "docs": [
                "Wormhole bridge data account (a.k.a. its config).",
                "[`wormhole::post_message`] requires this account be mutable."
              ],
              "writable": true,
              "pda": {
                "seeds": [
                  {
                    "kind": "const",
                    "value": [
                      66,
                      114,
                      105,
                      100,
                      103,
                      101
                    ]
                  }
                ]
              }
            },
            {
              "name": "feeCollector",
              "docs": [
                "Wormhole fee collector account, which requires lamports before the",
                "program can post a message (if there is a fee).",
                "[`wormhole::post_message`] requires this account be mutable."
              ],
              "writable": true,
              "pda": {
                "seeds": [
                  {
                    "kind": "const",
                    "value": [
                      102,
                      101,
                      101,
                      95,
                      99,
                      111,
                      108,
                      108,
                      101,
                      99,
                      116,
                      111,
                      114
                    ]
                  }
                ]
              }
            },
            {
              "name": "sequence",
              "docs": [
                "message is posted, so it needs to be an [UncheckedAccount] for the",
                "[`initialize`](crate::initialize) instruction.",
                "[`wormhole::post_message`] requires this account be mutable."
              ],
              "writable": true,
              "pda": {
                "seeds": [
                  {
                    "kind": "const",
                    "value": [
                      83,
                      101,
                      113,
                      117,
                      101,
                      110,
                      99,
                      101
                    ]
                  },
                  {
                    "kind": "account",
                    "path": "config"
                  }
                ]
              }
            },
            {
              "name": "message",
              "docs": [
                "account be mutable."
              ],
              "writable": true,
              "signer": true
            },
            {
              "name": "payer",
              "writable": true,
              "signer": true
            },
            {
              "name": "clock",
              "address": "SysvarC1ock11111111111111111111111111111111"
            },
            {
              "name": "rent",
              "address": "SysvarRent111111111111111111111111111111111"
            },
            {
              "name": "wormholeProgram",
              "docs": [
                "Wormhole program."
              ],
              "address": "worm2ZoG2kUd4vFXhvjh93UUH596ayRfgQ2MgjNMTth"
            },
            {
              "name": "systemProgram",
              "address": "11111111111111111111111111111111"
            }
          ]
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        },
        {
          "name": "tokenProgram"
        },
        {
          "name": "associatedTokenProgram",
          "address": "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
        }
      ],
      "args": [
        {
          "name": "metadataOverride",
          "type": {
            "defined": {
              "name": "metadataOverride"
            }
          }
        }
      ]
    }
  ],
  "accounts": [
    {
      "name": "config",
      "discriminator": [
        155,
        12,
        170,
        224,
        30,
        250,
        204,
        130
      ]
    },
    {
      "name": "usedNonces",
      "discriminator": [
        60,
        112,
        18,
        72,
        138,
        181,
        100,
        138
      ]
    }
  ],
  "errors": [
    {
      "code": 6000,
      "name": "invalidArgs",
      "msg": "Invalid arguments"
    },
    {
      "code": 6001,
      "name": "signatureVerificationFailed",
      "msg": "Signature verification failed"
    },
    {
      "code": 6002,
      "name": "nonceAlreadyUsed"
    },
    {
      "code": 6003,
      "name": "unauthorized"
    },
    {
      "code": 6004,
      "name": "tokenMetadataNotProvided"
    },
    {
      "code": 6005,
      "name": "solanaTokenParsingFailed"
    }
  ],
  "types": [
    {
      "name": "config",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "admin",
            "type": "pubkey"
          },
          {
            "name": "maxUsedNonce",
            "type": "u128"
          },
          {
            "name": "derivedNearBridgeAddress",
            "type": {
              "array": [
                "u8",
                64
              ]
            }
          },
          {
            "name": "bumps",
            "type": {
              "defined": {
                "name": "configBumps"
              }
            }
          }
        ]
      }
    },
    {
      "name": "configBumps",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "config",
            "type": "u8"
          },
          {
            "name": "authority",
            "type": "u8"
          },
          {
            "name": "wormhole",
            "type": {
              "defined": {
                "name": "wormholeBumps"
              }
            }
          }
        ]
      }
    },
    {
      "name": "deployTokenPayload",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "token",
            "type": "string"
          },
          {
            "name": "name",
            "type": "string"
          },
          {
            "name": "symbol",
            "type": "string"
          },
          {
            "name": "decimals",
            "type": "u8"
          }
        ]
      }
    },
    {
      "name": "finalizeTransferPayload",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "nonce",
            "type": "u128"
          },
          {
            "name": "amount",
            "type": "u128"
          },
          {
            "name": "feeRecipient",
            "type": {
              "option": "string"
            }
          }
        ]
      }
    },
    {
      "name": "initTransferPayload",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "amount",
            "type": "u128"
          },
          {
            "name": "recipient",
            "type": "string"
          },
          {
            "name": "fee",
            "type": "u128"
          }
        ]
      }
    },
    {
      "name": "metadataOverride",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "name",
            "type": "string"
          },
          {
            "name": "symbol",
            "type": "string"
          }
        ]
      }
    },
    {
      "name": "signedPayload",
      "generics": [
        {
          "kind": "type",
          "name": "p"
        }
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "payload",
            "type": {
              "generic": "p"
            }
          },
          {
            "name": "signature",
            "type": {
              "array": [
                "u8",
                65
              ]
            }
          }
        ]
      }
    },
    {
      "name": "usedNonces",
      "serialization": "bytemuckunsafe",
      "repr": {
        "kind": "c"
      },
      "type": {
        "kind": "struct",
        "fields": []
      }
    },
    {
      "name": "wormholeBumps",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "bridge",
            "type": "u8"
          },
          {
            "name": "feeCollector",
            "type": "u8"
          },
          {
            "name": "sequence",
            "type": "u8"
          }
        ]
      }
    }
  ],
  "constants": [
    {
      "name": "authoritySeed",
      "type": "bytes",
      "value": "[97, 117, 116, 104, 111, 114, 105, 116, 121]"
    },
    {
      "name": "configSeed",
      "type": "bytes",
      "value": "[99, 111, 110, 102, 105, 103]"
    },
    {
      "name": "solanaOmniBridgeChainId",
      "type": "u8",
      "value": "2"
    },
    {
      "name": "usedNoncesAccountSize",
      "type": "u32",
      "value": "136"
    },
    {
      "name": "usedNoncesPerAccount",
      "type": "u32",
      "value": "1024"
    },
    {
      "name": "usedNoncesSeed",
      "type": "bytes",
      "value": "[117, 115, 101, 100, 95, 110, 111, 110, 99, 101, 115]"
    },
    {
      "name": "vaultSeed",
      "type": "bytes",
      "value": "[118, 97, 117, 108, 116]"
    },
    {
      "name": "wrappedMintSeed",
      "type": "bytes",
      "value": "[119, 114, 97, 112, 112, 101, 100, 95, 109, 105, 110, 116]"
    }
  ]
};
