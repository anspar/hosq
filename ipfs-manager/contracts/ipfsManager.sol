// SPDX-License-Identifier: GPL-3.0
pragma solidity >=0.8.0 <0.9.0;

contract IpfsManager {

  address private deployer;
  mapping(string=>uint256) private cid_valid_block;
  uint256 private per_block_price = 100*1e9; // 100 Gwei

  event UpdateValidBlock(uint256 update_block, uint256 end_block, string cid);

  modifier onlyOwner() {
        require(msg.sender == deployer, "Not the deployer");
        _;
    }

  function add_new_valid_block(string memory _cid) public payable{
    uint256 num_blocks = get_num_of_valid_blocks(msg.value);
    require(num_blocks>0, "not enought funds");
    uint curBlock = block.number;
    if(cid_valid_block[_cid]>curBlock){
      cid_valid_block[_cid] += num_blocks; 
    }else{
      cid_valid_block[_cid] = curBlock+num_blocks;
    } 
    emit UpdateValidBlock(curBlock, cid_valid_block[_cid], _cid);
  }

  function get_valid_block(string memory _cid) public view returns(uint256){
    return cid_valid_block[_cid];
  }

  function get_num_of_valid_blocks(uint256 _price_wei) public view returns(uint256){
    if (_price_wei<per_block_price) return 0;
    return _price_wei/per_block_price;
  }

  function update_per_block_price(uint256 _price_wei) public{
    per_block_price = _price_wei;
  }

  function get_per_block_price() public view returns(uint256){
    return per_block_price;
  }

  function withdraw() public onlyOwner{
        // get the amount of Ether stored in this contract
        uint amount = address(this).balance;

        // send all Ether to deployer
        (bool success,) = deployer.call{value: amount}("");
        require(success, "Failed to withdraw balance");
    }
  
  constructor() {
    deployer = msg.sender;
  }
}
