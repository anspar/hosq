// SPDX-License-Identifier: GPL-3.0
pragma solidity >=0.8.0 <0.9.0;

import "@openzeppelin/contracts/utils/math/SafeMath.sol";
import "@openzeppelin/contracts/utils/Counters.sol";

contract IpfsManager {
  using SafeMath for uint256;
  using Counters for Counters.Counter;
  Counters.Counter private providers_id;

  struct Provider{
    address account;
    uint256 block_price;
    string api_url;
  }

  address private deployer;
  mapping(string=>mapping(uint256=>uint256)) private cid_valid_block;
  mapping(uint256=>Provider) private service_providers; // addresses to pay to for pinning services
  // uint256 private per_block_price = 100*1e9; // 100 Gwei
  uint24 decimals = 100000;
  uint24 developer_fee = 2500; //2.5%

  event UpdateValidBlock(address donor, uint256 end_block, uint256 provider_id, string cid);
  event AddProvider(address creator, uint256 id, uint256 block_price, string api_url);
  event UpdateProviderBlockPrice(uint256 id, uint256 price);
  event UpdateProviderApiUrl(uint256 id, string url);
  event UpdateProviderAddress(uint256 id, address account);

  modifier onlyOwner() {
        require(msg.sender == deployer, "Not the deployer");
        _;
    }

  function add_pinning_service(string memory _api_url, uint256 _block_price) public{
    require(_block_price>decimals, "block price <= 100000");
    providers_id.increment();
    uint256 id = providers_id.current();
    service_providers[id] = Provider(msg.sender, _block_price, _api_url);
    emit AddProvider(msg.sender, id, _block_price, _api_url);
  }

  function add_new_valid_block(string memory _cid, uint256 _num_blocks, uint256 _provider_id) public payable{
    require(_num_blocks>0, "blocks=0");
    (uint256 total_price, uint256 fee) = get_total_price_for_blocks(_num_blocks, _provider_id);
    require(msg.value>=total_price.add(fee));
     
    (bool success_provider,) = service_providers[_provider_id].account.call{value: total_price}("");
    require(success_provider, "Failed to send provider fee");
    
    uint curBlock = block.number;
    if(cid_valid_block[_cid][_provider_id]>curBlock){
      cid_valid_block[_cid][_provider_id] += _num_blocks; 
    }else{
      cid_valid_block[_cid][_provider_id] = curBlock+_num_blocks;
    } 
    
    emit UpdateValidBlock(msg.sender, cid_valid_block[_cid][_provider_id], _provider_id, _cid);
  }

  function get_valid_block(string memory _cid, uint256 _provider_id) public view returns(uint256){
    return cid_valid_block[_cid][_provider_id];
  }

  function get_total_price_for_blocks(uint256 _blocks, uint256 _provider_id) public view returns(uint256, uint256){
    uint256 price_for_blocks = _blocks.mul(service_providers[_provider_id].block_price);
    require(price_for_blocks>decimals, "total<100000");
    return (price_for_blocks,  price_for_blocks.mul(developer_fee).div(decimals));
  }

  function update_block_price(uint256 _price_wei, uint256 _provider_id) public{
    require(msg.sender==service_providers[_provider_id].account, "wrong address");
    require(_price_wei>decimals, "block price <= 100000");
    service_providers[_provider_id].block_price = _price_wei;
    emit UpdateProviderBlockPrice(_provider_id, _price_wei);
  }

  function update_api_url(string memory _url, uint256 _provider_id) public{
    require(msg.sender==service_providers[_provider_id].account, "wrong address");
    service_providers[_provider_id].api_url = _url;
    emit UpdateProviderApiUrl(_provider_id, _url);
  }

  function update_address(address _new, uint256 _provider_id) public{
    require(msg.sender==service_providers[_provider_id].account, "wrong address");
    service_providers[_provider_id].account = _new;
    emit UpdateProviderAddress(_provider_id, _new);
  }

  function get_per_block_price(uint256 _provider_id) public view returns(uint256){
    return service_providers[_provider_id].block_price;
  }

  function withdraw() public onlyOwner{
        // get the amount of Ether stored in this contract
        uint amount = address(this).balance;

        // send all Ether to deployer
        (bool success,) = deployer.call{value: amount}("");
        require(success, "Failed to withdraw balance");
  }

  function update_dev_fee(uint24 _fee) public onlyOwner{
    developer_fee = _fee;
  }
  
  constructor() {
    deployer = msg.sender;
  }
}
