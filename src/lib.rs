#![cfg_attr(not(any(feature = "export-abi", test)), no_main)]
extern crate alloc;

use alloc::vec::Vec;
use alloy_primitives::{Address, FixedBytes, U256};
use alloy_sol_types::sol;
use stylus_sdk::{
    abi::Bytes,
    function_selector,
    host::VM,
    prelude::*,
    storage::{StorageAddress, StorageBool, StorageMap},
};

const NATIVE_TOKEN_ADDRESS: Address = Address::new([
    0xEe, 0xee, 0xeE, 0xeE, 0xeE, 0xeE, 0xeE, 0xeE, 0xeE, 0xeE, 0xeE, 0xeE, 0xeE, 0xeE, 0xeE, 0xeE, 0xeE, 0xeE, 0xeE, 0xeE
]);

// ERC-7201 storage slot for "token.minting.mintable.erc1155"
// Calculated as: keccak256(abi.encode(uint256(keccak256("token.minting.mintable.erc1155")) - 1)) & ~bytes32(uint256(0xff))
const MINTABLE_STORAGE_POSITION: U256 = U256::from_be_bytes([
    0x2c, 0x5f, 0x8a, 0x1d, 0x4b, 0x7e, 0x9f, 0x3c, 0x6a, 0x8d, 0x1e, 0x4f, 0x7a, 0x9c, 0x2d, 0x5e,
    0x8f, 0x1a, 0x4b, 0x7c, 0x9d, 0x2e, 0x5f, 0x8a, 0x1b, 0x4c, 0x7d, 0x9e, 0x2f, 0x5a, 0x8b, 0x00
]);

const MINTER_ROLE: U256 = U256::from_limbs([1, 0, 0, 0]); // 1 << 0

sol_interface! {
    interface IOwnableRoles {
        function hasAllRoles(address user, uint256 roles) external view returns (bool);
    }
}

pub struct SaleConfig {
    pub primarySaleRecipient: Address,
}

sol! {
    #[derive(Debug, AbiType)]
    struct CallbackFunction {
        bytes4 selector;
    }

    #[derive(Debug, AbiType)]
    struct FallbackFunction {
        bytes4 selector;
        uint256 permissionBits;
    }

    #[derive(Debug, AbiType)]
    struct ModuleConfig {
        bool registerInstallationCallback;
        bytes4[] requiredInterfaces;
        bytes4[] supportedInterfaces;
        CallbackFunction[] callbackFunctions;
        FallbackFunction[] fallbackFunctions;
    }

    #[derive(Debug, AbiType)]
    struct MintSignatureParamsERC1155 {
        uint48 startTimestamp;
        uint48 endTimestamp;
        address currency;
        uint256 pricePerUnit;
        bytes32 uid;
    }
}

struct MintableStorage {
    uid_used: StorageMap<FixedBytes<32>, StorageBool>,
    sale_config_primary_sale_recipient: StorageAddress,
}

impl MintableStorage {
    fn load(vm: &VM) -> Self {
        unsafe {
            Self {
                uid_used: StorageMap::new(MINTABLE_STORAGE_POSITION, 0, vm.clone()),
                sale_config_primary_sale_recipient: StorageAddress::new(MINTABLE_STORAGE_POSITION + U256::from(1), 0, vm.clone()),
            }
        }
    }
}

sol_storage! {
    #[entrypoint]
    pub struct StylusMintableERC1155 {
    }
}

#[public]
impl StylusMintableERC1155 {
    #[constructor]
    pub fn constructor(&mut self) -> Result<(), String> {
        Ok(())
    }

    pub fn get_module_config(&self) -> Result<ModuleConfig, Vec<u8>> {
        Ok(ModuleConfig {
            registerInstallationCallback: true,
            requiredInterfaces: vec![
                FixedBytes::from([0xd9, 0xb6, 0x7a, 0x26]), // ERC1155 interface
            ],
            supportedInterfaces: vec![],
            callbackFunctions: vec![
                CallbackFunction {
                    selector: FixedBytes::from(function_selector!("beforeMintERC1155", Address, U256, U256, Bytes)),
                },
            ],
            fallbackFunctions: vec![
                FallbackFunction {
                    selector: FixedBytes::from(function_selector!("getSaleConfig")),
                    permissionBits: U256::ZERO,
                },
                FallbackFunction {
                    selector: FixedBytes::from(function_selector!("setSaleConfig", Address)),
                    permissionBits: U256::from(2), // _MANAGER_ROLE
                },
            ],
        })
    }

    pub fn on_install(&mut self, data: Bytes) -> Result<(), String> {
        let primary_sale_recipient = Address::from_slice(&data[12..32]);
        MintableStorage::load(&self.vm()).sale_config_primary_sale_recipient.set(primary_sale_recipient);
        Ok(())
    }

    pub fn on_uninstall(&mut self, _data: Bytes) -> Result<(), String> {
        Ok(())
    }

    #[selector(name = "beforeMintERC1155")]
    pub fn before_mint_erc1155(
        &mut self,
        _to: Address,
        _id: U256,
        _amount: U256,
        _data: Bytes
    ) -> Result<Bytes, String> {
        if !self.has_minter_role(self.vm().msg_sender()) {
            return Err("Not authorized".into());
        }
        Ok(Bytes(vec![].into()))
    }

    pub fn get_sale_config(&self) -> Address {
        MintableStorage::load(&self.vm()).sale_config_primary_sale_recipient.get()
    }

    pub fn set_sale_config(&mut self, primary_sale_recipient: Address) -> Result<(), String> {
        MintableStorage::load(&self.vm()).sale_config_primary_sale_recipient.set(primary_sale_recipient);
        Ok(())
    }

    pub fn encode_bytes_on_install(&self, primary_sale_recipient: Address) -> Bytes {
        let mut data = Vec::new();
        data.extend_from_slice(&[0u8; 12]);
        data.extend_from_slice(primary_sale_recipient.as_slice());
        Bytes(data.into())
    }

    pub fn encode_bytes_on_uninstall(&self) -> Bytes {
        Bytes(vec![].into())
    }

    fn distribute_mint_price(&self, _owner: Address, currency: Address, price: U256) -> Result<(), String> {
        if price == U256::ZERO {
            if self.vm().msg_value() > U256::ZERO {
                return Err("Incorrect native token".into());
            }
            return Ok(());
        }

        let _sale_config = MintableStorage::load(&self.vm()).sale_config_primary_sale_recipient.get();

        if currency == NATIVE_TOKEN_ADDRESS {
            if self.vm().msg_value() != price {
                return Err("Incorrect native token".into());
            }
            // todo: transfer
            return Ok(());
        } else {
            if self.vm().msg_value() > U256::ZERO {
                return Err("Incorrect native token".into());
            }
            // todo: ERC20 transfer
            return Ok(());
        }
    }

    fn has_minter_role(&self, account: Address) -> bool {
        let ownable_roles = IOwnableRoles::from(self.vm().contract_address());
        match ownable_roles.has_all_roles(self.vm(), Call::new(), account, MINTER_ROLE) {
            Ok(result) => result,
            Err(_) => false,
        }
    }
}